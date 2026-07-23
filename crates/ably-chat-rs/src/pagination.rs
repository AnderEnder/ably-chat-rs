//! Cursor pagination as `Page<T>` + `Stream` over RFC 5988 `Link` headers
//! (ADR-0009). The dispatch layer surfaces the response headers the generated
//! functions discard; this module parses the `rel="next"` cursor and follows it.

use futures::Stream;
use reqwest::Method;
use reqwest::header::{HeaderMap, LINK};
use serde::de::DeserializeOwned;

use crate::client::Client;
use crate::dispatch::{RawResponse, decode_json};
use crate::error::Result;

/// One page of a paginated collection: the decoded items plus the parsed
/// `rel="next"` cursor (ADR-0009).
///
/// Obtain the next page manually with [`next`](Self::next), or follow the whole
/// chain lazily with [`into_stream`](Self::into_stream).
#[derive(Clone, Debug)]
pub struct Page<T> {
    items: Vec<T>,
    /// Absolute URL of the next page, if any (already resolved against the base).
    next: Option<String>,
    client: Client,
}

impl<T> Page<T> {
    /// The items on this page.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Consumes the page, returning its items.
    pub fn into_items(self) -> Vec<T> {
        self.items
    }

    /// Whether a further page is available via [`next`](Self::next).
    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }
}

impl<T: DeserializeOwned + Send + 'static> Page<T> {
    /// Fetches the first page for a base-relative `path` and `query`. The shared
    /// entry point for both the `.await` (single page) and `.into_stream()`
    /// forms of the paginated builders, so the query is built in exactly one
    /// place.
    pub(crate) async fn fetch_first(
        client: Client,
        path: String,
        query: Vec<(&'static str, String)>,
    ) -> Result<Self> {
        let resp = client
            .inner
            .send(Method::GET, &path, &query, None, false)
            .await?;
        Self::from_response(client, &resp)
    }

    /// Builds a page from a raw response, decoding the JSON array body and
    /// resolving the `next` link (if present) to an absolute URL.
    fn from_response(client: Client, resp: &RawResponse) -> Result<Self> {
        let items = decode_json::<Vec<T>>(&resp.body)?;
        let next =
            parse_next_link(&resp.headers).map(|link| resolve_url(&client.inner.base, &link));
        Ok(Self {
            items,
            next,
            client,
        })
    }

    /// Fetches the next page, or `None` when the current page is the last.
    pub async fn next(&self) -> Result<Option<Page<T>>> {
        match &self.next {
            None => Ok(None),
            Some(url) => {
                let resp = self
                    .client
                    .inner
                    .send_url(Method::GET, url.clone(), &[], None, false)
                    .await?;
                Ok(Some(Self::from_response(self.client.clone(), &resp)?))
            }
        }
    }

    /// Streams every item across all pages, following `next` links until the
    /// collection is exhausted. Starts from this page's already-fetched items.
    pub fn into_stream(self) -> impl Stream<Item = Result<T>> + Send {
        let fetch = match self.next {
            Some(url) => Fetch::Follow(url),
            None => Fetch::Stop,
        };
        run_stream(self.client, self.items, fetch)
    }
}

/// What the paginating stream should fetch when its current buffer is drained.
pub(crate) enum Fetch {
    /// Fetch the first page from a base-relative path + query.
    First {
        path: String,
        query: Vec<(&'static str, String)>,
    },
    /// Follow an absolute `next` URL.
    Follow(String),
    /// No more pages.
    Stop,
}

/// Drives the shared paginating stream: yields buffered items, then fetches the
/// next page (first request or `next` link) when the buffer empties, stopping
/// on the first error (yielded once) or when there is no further page.
pub(crate) fn run_stream<T>(
    client: Client,
    initial: Vec<T>,
    fetch: Fetch,
) -> impl Stream<Item = Result<T>> + Send
where
    T: DeserializeOwned + Send + 'static,
{
    struct State<T> {
        client: Client,
        buffer: std::vec::IntoIter<T>,
        fetch: Fetch,
        done: bool,
    }

    let state = State {
        client,
        buffer: initial.into_iter(),
        fetch,
        done: false,
    };

    futures::stream::unfold(state, |mut st| async move {
        if st.done {
            return None;
        }
        loop {
            if let Some(item) = st.buffer.next() {
                return Some((Ok(item), st));
            }
            let resp = match std::mem::replace(&mut st.fetch, Fetch::Stop) {
                Fetch::Stop => return None,
                Fetch::First { path, query } => {
                    st.client
                        .inner
                        .send(Method::GET, &path, &query, None, false)
                        .await
                }
                Fetch::Follow(url) => {
                    st.client
                        .inner
                        .send_url(Method::GET, url, &[], None, false)
                        .await
                }
            };
            match resp.and_then(|r| Page::<T>::from_response(st.client.clone(), &r)) {
                Ok(page) => {
                    st.buffer = page.items.into_iter();
                    st.fetch = match page.next {
                        Some(url) => Fetch::Follow(url),
                        None => Fetch::Stop,
                    };
                }
                Err(e) => {
                    st.done = true;
                    return Some((Err(e), st));
                }
            }
        }
    })
}

/// Parses the `rel="next"` target from the response `Link` headers (RFC 5988).
///
/// Handles both one header per relation and a single header carrying several
/// comma-separated links. Returns the raw (possibly relative) URL.
pub(crate) fn parse_next_link(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(LINK)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .find_map(|link| {
            let (url_part, params) = link.split_once(';')?;
            rel_is_next(params).then(|| {
                url_part
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim()
                    .to_owned()
            })
        })
}

/// Whether any `;`-separated parameter is `rel="next"` (case-insensitive, with
/// or without surrounding quotes).
fn rel_is_next(params: &str) -> bool {
    params.split(';').any(|p| match p.split_once('=') {
        Some((k, v)) => {
            k.trim().eq_ignore_ascii_case("rel")
                && v.trim().trim_matches('"').eq_ignore_ascii_case("next")
        }
        None => false,
    })
}

/// Resolves a `Link` target against the client base: absolute URLs pass through,
/// base-relative paths are prefixed with `base`.
fn resolve_url(base: &str, link: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") {
        link.to_owned()
    } else {
        format!("{base}{link}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Client;
    use crate::config::Auth;
    use crate::dispatch::room_path;
    use crate::types::Message;
    use futures::StreamExt;
    use reqwest::header::{HeaderMap, HeaderValue, LINK};
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn message_json(serial: &str) -> String {
        format!(
            r#"{{"serial":"{serial}","version":{{"serial":"{serial}","timestamp":1}},
               "text":"t","clientId":"a","action":"message.create",
               "metadata":{{}},"headers":{{}},"timestamp":1}}"#
        )
    }

    #[test]
    fn parses_next_link_and_ignores_other_rels() {
        let mut headers = HeaderMap::new();
        headers.append(
            LINK,
            HeaderValue::from_static("</chat/v4/rooms/r/messages?cont=first>; rel=\"first\""),
        );
        headers.append(
            LINK,
            HeaderValue::from_static("</chat/v4/rooms/r/messages?cont=2>; rel=\"next\""),
        );
        assert_eq!(
            parse_next_link(&headers).as_deref(),
            Some("/chat/v4/rooms/r/messages?cont=2")
        );

        // A single header carrying several comma-separated links also parses.
        let mut combined = HeaderMap::new();
        combined.append(
            LINK,
            HeaderValue::from_static("</m?cont=cur>; rel=\"current\", </m?cont=n>; rel=\"next\""),
        );
        assert_eq!(parse_next_link(&combined).as_deref(), Some("/m?cont=n"));

        // No next relation → None.
        let mut only_first = HeaderMap::new();
        only_first.append(LINK, HeaderValue::from_static("</m?cont=x>; rel=\"first\""));
        assert_eq!(parse_next_link(&only_first), None);
    }

    #[test]
    fn resolves_relative_and_absolute_links() {
        assert_eq!(
            resolve_url("https://rest.ably.io", "/chat/v4/rooms/r/messages?cont=2"),
            "https://rest.ably.io/chat/v4/rooms/r/messages?cont=2"
        );
        assert_eq!(
            resolve_url("https://rest.ably.io", "https://other.host/x?cont=2"),
            "https://other.host/x?cont=2"
        );
    }

    #[tokio::test]
    async fn manual_next_walks_two_pages_then_none() {
        let server = MockServer::start().await;
        // Page 1: no continuation param; emits a relative `next` link.
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/messages"))
            .and(query_param_is_missing("cont"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Link", "</chat/v4/rooms/r/messages?cont=2>; rel=\"next\"")
                    .set_body_string(format!("[{}]", message_json("m1"))),
            )
            .mount(&server)
            .await;
        // Page 2: continuation param present; no `next` link → last page.
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/messages"))
            .and(query_param("cont", "2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("[{}]", message_json("m2"))),
            )
            .mount(&server)
            .await;

        let client = Client::builder(Auth::api_key("k:s"))
            .host(server.uri())
            .build();
        let page1: Page<Message> =
            Page::fetch_first(client.clone(), room_path("r", "/messages"), Vec::new())
                .await
                .unwrap();
        assert_eq!(page1.items().len(), 1);
        assert_eq!(page1.items()[0].serial.as_str(), "m1");
        assert!(page1.has_next());

        let page2 = page1.next().await.unwrap().expect("expected a second page");
        assert_eq!(page2.items().len(), 1);
        assert_eq!(page2.items()[0].serial.as_str(), "m2");
        assert!(!page2.has_next());

        assert!(page2.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn into_stream_yields_all_items_across_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/messages"))
            .and(query_param_is_missing("cont"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Link", "</chat/v4/rooms/r/messages?cont=2>; rel=\"next\"")
                    .set_body_string(format!("[{}]", message_json("m1"))),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/messages"))
            .and(query_param("cont", "2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("[{}]", message_json("m2"))),
            )
            .mount(&server)
            .await;

        let client = Client::builder(Auth::api_key("k:s"))
            .host(server.uri())
            .build();
        let page1: Page<Message> =
            Page::fetch_first(client.clone(), room_path("r", "/messages"), Vec::new())
                .await
                .unwrap();
        let serials: Vec<String> = page1
            .into_stream()
            .map(|r| r.unwrap().serial.as_str().to_owned())
            .collect()
            .await;
        assert_eq!(serials, vec!["m1", "m2"]);
    }
}
