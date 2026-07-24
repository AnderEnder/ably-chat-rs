# Ably Chat permissions & auth — research and implementation design for `ably-chat-rs`

- Date: 2026-07-24
- Author: research synthesis (deep-research, exhaustive mode)
- Status: research memo — informs a future ADR; not itself a decision
- Scope: Ably platform + Ably Chat **v4**; auth mechanisms, the capabilities
  (permissions) model, Chat's room→channel mapping, token lifecycle, Rust prior
  art, and a concrete design for what (if anything) `ably-chat-rs` should build.
- Update — iteration 2 (2026-07-24): see the **Addendum** at the end for the
  permission-management playbook (roles, moderation, mute/ban), the **resolved**
  room-resource question, the per-endpoint capability map, revocation, Control-API
  reuse, `ably`-crate interop, and a competitor comparison. Where the addendum and
  this body differ, **the addendum is newer and wins** (notably the room-resource
  scoping in Finding 3 and Appendix B, corrected below).

---

## Executive summary

Ably has **one** permission model, and it is *not* chat-specific: every operation
is gated by **capabilities** — a JSON map from channel-name patterns to a list of
allowed **operations** — attached either to a long-lived **API key** (HTTP Basic
auth, server-side only) or to a short-lived **token** (Ably Token or Ably JWT, sent
as `Authorization: Bearer`). Ably Chat rides entirely on this: a Chat *room* named
`R` is backed by a single Pub/Sub channel `R::$chat`, and each Chat feature maps to
ordinary channel operations (`publish`, `subscribe`, `history`, `message-update-*`,
`message-delete-*`, `annotation-publish`, `channel-metadata`, `presence`). There is
**no** Chat-specific capability namespace, **no** `moderation` operation, and **no**
role system — "moderator" is simply a token holding the elevated `message-*-any`
operations instead of `message-*-own`.

The crate today (ADR-0005) accepts a static API key or a static token and does
nothing else with auth. "Implementing permissions/auth" could therefore mean four
distinct things, and they differ sharply in value, risk, and architectural fit:

1. **Typed capability model** — represent the capability document in Rust. High
   value (no Rust crate has this today), zero secret-handling risk, small.
2. **Ably JWT minting** — sign a short-lived JWT with the API secret for handing to
   end-user clients. Officially recommended pattern, no network call, and (crucially)
   **no capability canonicalization needed**. Server-side-only; moderate risk.
3. **Token auto-refresh (`TokenProvider`)** — the extension point ADR-0005 already
   reserved. Client-side lifecycle plumbing; moderate complexity.
4. **Native Ably Token minting** (`POST /keys/{keyName}/requestToken`, with HMAC
   TokenRequest signing) — **decline.** It is a *platform* REST call outside the
   `/chat/v4` surface, it is the fiddliest/most footgun-prone piece, and the official
   `ably` Rust crate already implements it.

**Overall recommendation:** keep `ably-chat-rs` a Chat-REST client. Add (behind
feature flags) a **typed capability model** and an **Ably JWT minter** as
server-side token-issuance helpers, and land the reserved **`Auth::Provider`
refresh seam**. Do *not* reimplement TokenRequest signing or the platform
`requestToken` exchange — defer those to the official `ably` crate via a clean
interop story. One correctness question is a hard blocker for the capability helper
and must be verified empirically first (see **Finding 3 / the room-resource trap**).

Biggest risk / gap: the exact capability **resource key** that authorizes a chat room
looked self-contradictory in Ably's docs; iteration-2 research **resolved** it — scope by
**room name** (bare `"{room}"` or `"[chat]{room}"`), **not** `"{room}::$chat"` — with a
live test matrix still recommended (Addendum §A2.2). Everything else is confirmed against
primary source.

---

## Methodology

**Research period:** July 2026.
**Mode:** Exhaustive — six parallel research agents (five topic scouts + a
contrarian) followed by one citation-audit/verification round.

**Source strategy:** primary sources only for load-bearing claims — SDK source on
GitHub (`ably/ably-js`, `ably/ably-chat-js`, `ably/ably-rust`, `ably/ably-go`), the
machine-readable specs (`ably/open-specs` `platform-v1.yaml`, the Ably client-library
features spec / RSA IDs), and the `ably/docs` main branch (read as raw markdown to
avoid doc-site summarization error). Secondary sources (rendered doc pages, OWASP
mobile guidance) were used only for corroboration or for the security argument.

**Verification:** all six top claims (TokenRequest signing string, `c14n`
canonicalization, room→channel name, feature→capability table, the 17-operation
list, the renewal-trigger range) were independently re-verified in a dedicated audit
pass against source line references. One cross-source conflict (the `requestToken`
host) was surfaced and resolved.

**Queries discarded:** rendered `ably.com/docs/*` HTML routes where a summarizer sat
in front of the content (replaced with raw GitHub markdown); `ably.com/docs/api/chat-rest.md`
(HTTP 404 — Chat REST specifics taken from `ably-chat-js` source and this repo's
reconstructed OpenAPI instead).

**Known limitations before synthesis:** Ably publishes no per-REST-endpoint
capability spec, so the mapping from a specific `/chat/v4` endpoint to its required
operation is partly inferred (labeled where so). Ably doc pages are undated living
documents.

---

## Background / context

**Why this matters.** `ably-chat-rs` is an unofficial, ergonomic, **REST-only** Rust
client for the Ably Chat REST API (`/chat/v4/*`), deliberately scoped away from
realtime (ADR-0001) and, for auth, deliberately limited to **static credentials**
(ADR-0005): the caller passes either an API key (rendered as HTTP Basic) or a
pre-obtained token/JWT (rendered as Bearer). `config.rs` models this as
`enum Auth { ApiKey(String), Token(String) }` and precomputes a single
`Authorization` header string at build time; a `TODO` reserves a future
`Auth::Provider(Arc<dyn TokenProvider>)` for refresh. Any work on "permissions/auth"
starts from this baseline.

**Key definitions (used throughout):**

- **API key** — `keyName:keySecret`, where `keyName` is itself `appId.keyId`. Long-lived,
  never expires, carries a fixed capability set configured in the dashboard. Basic auth.
- **Ably Token / TokenDetails** — a short-lived credential issued by Ably; the `token`
  string is presented as `Authorization: Bearer <token>`. Fields: `token`, `keyName`,
  `issued`, `expires`, `capability`, `clientId`.
- **Ably TokenRequest** — a request object an app server signs (HMAC-SHA256 with the
  key secret) and hands to a client or posts to `requestToken`; it is *not itself a
  usable credential* — it must be exchanged for a Token.
- **Ably JWT** — a standard JWT (HS256, `kid` = key name, signed with the key secret)
  carrying `x-ably-capability` / `x-ably-clientId` claims. Directly presentable as a
  Bearer credential. No Ably SDK or network call required to mint one.
- **Capability** — a JSON map `{ resourcePattern: [operation, …], … }`. The unit of
  permission in Ably.

**Current state of the field (Rust).** There is exactly one Rust Ably client: the
official `ably` crate (`ably/ably-rust`), v0.2.0, REST-only, "Developer Preview" but
actively maintained (last pushed 2026-07-20). It implements a `Credential` enum
(`Key`, `TokenDetails`, `TokenRequest`, `Callback`, `Url`), `request_token`, local
`create_token_request` (HMAC signing), and an `AuthCallback` trait — but it models
capabilities as a raw `String` and does **not** mint Ably JWTs (it only accepts them
opaquely). No third-party/community Rust Ably client exists. So: typed capabilities
and JWT minting are **greenfield** in the Rust ecosystem.

---

## Stakeholder / actor map

| Actor | Role | Interest / relevance |
|---|---|---|
| **App server (trusted)** | Holds the API key; the primary user of a REST client | Wants either to call Chat with its key directly, or to mint scoped tokens for its end users. This is `ably-chat-rs`'s dominant caller. |
| **End-user client (untrusted)** | Browser / mobile / realtime | Must never see the API key; uses a short-lived token/JWT with a narrowed capability + a `clientId`. Generally a *realtime* consumer, not a REST one. |
| **Ably platform** | Enforces capabilities server-side; issues tokens at `POST /keys/{keyName}/requestToken` on `rest.ably.io` | The source of truth for permission enforcement; the boundary between "chat" and "platform" APIs. |
| **`ably-chat-rs` (this crate)** | Chat-REST ergonomics | Must decide how much of the auth/permission surface to own vs. delegate. |
| **Official `ably` crate** | Platform REST + full auth (signing, callbacks) | The natural home for TokenRequest signing and the `requestToken` exchange; an interop target rather than something to reimplement. |

---

## Findings by sub-question

### Sub-question 1 — Authentication mechanisms

**Evidence (confirmed, primary source):**

- **Basic auth**: `Authorization: Basic base64("keyName:keySecret")`. Declared in
  `platform-v1.yaml` as `basicAuth {type: http, scheme: basic}`. Grants the key's full
  configured capability; **server-side only** ("never use basic authentication
  client-side as it exposes your API key").
- **The Chat REST API accepts both schemes** — Basic (API key) *and* Bearer (an Ably
  Token string or an Ably JWT) — the same two schemes as the platform API, because
  Chat is channel-backed on top of Pub/Sub. (Confirmed against this repo's
  reconstructed OpenAPI, built from `@ably/chat` v1.4.0, and `configuration.rs`.)
- **TokenRequest HMAC signing** (verbatim from `ably-js` `auth.ts`, dual-confirmed
  against the token-request-spec):

  ```
  signText = keyName + '\n' + ttl + '\n' + capability + '\n'
           + clientId + '\n' + timestamp + '\n' + nonce + '\n'
  mac      = base64( HMAC_SHA256( utf8(signText), utf8(keySecret) ) )
  ```

  Field order is `keyName, ttl, capability, clientId, timestamp, nonce`; a newline
  (`0x0a`) follows **every** field including a trailing one after `nonce`; empty or
  absent fields become `''`. The field is singular **`capability`** (docs prose says
  "capabilities" loosely — the code is authoritative). The value is canonicalized
  first (see Finding 2). Endpoint path: `/keys/{keyName}/requestToken`.
- **Ably JWT**: header `{"typ":"JWT","alg":"HS256","kid":"<keyName>"}`, signed HS256
  with the key **secret**. Claims: `iat` (unix s, required), `exp` (unix s, required),
  `x-ably-capability` (JSON string, required), `x-ably-clientId` (optional). Any JWT
  library works; `x-ably-*` claims are reserved.
- **External JWT with embedded Ably token**: put an Ably token in the `x-ably-token`
  claim of your own JWT.
- **Host (conflict resolved):** the authoritative machine spec `platform-v1.yaml`
  declares a single server, **`https://rest.ably.io`** — which is exactly the base
  path this repo's `configuration.rs` already uses. Some rendered doc-UI examples show
  `main.realtime.ably.net`; `rest.ably.io` is the spec-backed answer. Token issuance
  (`/keys/…`) lives on this **platform** host, **distinct from `/chat/v4/*`**.
- **TTL & expiry codes:** default token TTL **60 min**; maxima — access token 24 h,
  revocable 1 h, push 5 y; exceeding → `40003`. Token error codes: `40140` token error
  (generic "not accepted"), `40141` revoked, `40142` **expired**, `40143` unrecognized,
  `40144` invalid JWT, `40145` invalid Ably token, `40160` capability denied, `40170`
  error from client token callback, `40171` renewal not configured.

**Conclusion.** The wire contract is fully pinned. For a REST-only client, the
practically relevant facts are: (a) Basic and Bearer are the only two schemes; (b) a
JWT can be minted locally with no network call; (c) native token *issuance* requires
the platform host, not `/chat/v4`. **Confidence: high.**

### Sub-question 2 — The capabilities / permissions model

**The 17 operations** (verbatim, `capabilities.mdx`, authoritative):
`subscribe`, `publish`, `presence`, `object-subscribe`, `object-publish`,
`annotation-subscribe`, `annotation-publish`, `message-update-own`,
`message-update-any`, `message-delete-own`, `message-delete-any`, `history`, `stats`,
`push-subscribe`, `push-admin`, `channel-metadata`, `privileged-headers`.

Traps worth internalizing:

- **There is no `presence-subscribe`.** `presence` grants only *registering your own*
  presence (enter/update/leave). *Receiving* others' presence and reading the presence
  set is bundled into **`subscribe`**.
- **There is no `moderation` operation and no role system.** "Moderator" = a token
  with `message-update-any` / `message-delete-any` (act on anyone's messages) instead
  of the `-own` variants (act only where the publisher's `clientId` matches).
- `stats` is meaningful only on `*` / `[*]*`; `channel-metadata` on `*` additionally
  enumerates channels.

**Resource scoping / wildcards:** a capability is a map from resource-name patterns to
operation lists; `:` delimits segments. `*` = any channel; a **trailing** wildcard
(`ns:*`) matches many segments; a **mid-name** wildcard (`a:*:b`) matches exactly one
segment; a wildcard not on a segment boundary (`foo*`) matches only the literal
channel `foo*`. Bracket qualifiers `[queue]…`, `[meta]…`, and `[*]*` (everything).
**There is no `[chat]` or `[product:…]` capability qualifier.**

**JSON format:** `{ "your-namespace:*": ["publish","subscribe","presence"], "notifications": ["subscribe"] }`;
full privilege is `{ "[*]*": ["*"] }`. In a JWT it is a **stringified** JSON value in
the `x-ably-capability` claim.

**Intersection:** a token's effective capability is the **intersection** of the
requested capability and the issuing key's own capability. Requesting a resource the
key does not cover is dropped silently; requesting *only* uncovered resources fails the
whole request ("The token request will fail if the intersection is empty"). Requesting
no capability at all defaults to `{"[*]*":["*"]}` → inherits all of the key's rights.

**`clientId` / identity:** an identified client (one with a trusted `clientId`) has that
id auto-populated and trusted on every operation and cannot masquerade as another. A
token binds the `clientId` server-side. The wildcard `clientId: "*"` is *privileged*
(assume any identity — for trusted backends); `"*"` cannot be a literal id; a
connection's `clientId` is immutable once set. Without a `clientId`, a token-authed
client cannot enter presence and its messages carry no `clientId`.

**Conclusion.** A faithful Rust model is `Map<resourcePattern, Set<operation>>` plus
the intersection semantics — not a flat set of operations. **Confidence: high.**

### Sub-question 3 — Chat-specific permission mapping

**Room → channel (confirmed from `ably-chat-js` `src/core/channel.ts`):**

```ts
export const roomChannelName = (roomName: string): string => `${roomName}::$chat`;
```

One channel per room, name = room name + the literal suffix `::$chat`. (Historically
Chat used several channels per room — `::$chatMessages`, `::$typingIndicators`,
`::$reactions` — since consolidated; `roomId` was also renamed `roomName`.)

**Feature → capability (verbatim, Chat `authentication.mdx`, confirmed):**

| Feature | Required operations |
|---|---|
| Send messages | `publish` |
| Receive messages | `subscribe` |
| Update messages | `message-update-any` **or** `message-update-own` |
| Delete messages | `message-delete-any` **or** `message-delete-own` |
| Message history | `subscribe`, `history` |
| Message reactions | `annotation-publish` (+ optionally `annotation-subscribe`) |
| Presence | `subscribe`, `presence` |
| Typing indicators | `publish`, `subscribe` |
| Room reactions | `publish`, `subscribe` |
| Occupancy | `subscribe`, `channel-metadata` |
| *All Chat features* | `publish`, `subscribe`, `presence`, `history`, `channel-metadata`, `annotation-publish`, `annotation-subscribe`, `message-update-own`, `message-delete-own` |

Note the "all features" row uses the **`-own`** variants — `-any` is a deliberately
separate, elevated moderator grant.

**REST vs realtime split** (from `ably-chat-js` `chat-api.ts`, base
`/chat/v4/rooms/{roomName}`). REST endpoints: `POST /messages` (send), `GET /messages`
(list/history), `GET /messages/{serial}`, `PUT /messages/{serial}` (update),
`POST /messages/{serial}/delete` (soft delete via POST), `POST|DELETE
/messages/{serial}/reactions`, `GET /messages/{serial}/client-reactions`,
`GET /messages/{serial}/versions`, `GET /occupancy`. **Realtime-only** (no REST
endpoint): the live message subscription, presence enter/update/leave, typing
indicators, and ephemeral room reactions.

**Implication for a REST-only crate (synthesis — carries an unverified assumption):**
a REST-only token almost certainly never needs `presence` (that gates realtime-only
enter/update/leave). The other REST-relevant operations are `publish` (send),
`history` (read/list/versions), `message-update-*` (update), `message-delete-*`
(delete), `annotation-publish` (add/remove reactions), and `channel-metadata`
(occupancy); a REST *moderation* client specifically needs the `-any` variants.
**Caveat — `subscribe` for reads:** Ably's own (verified) table pairs
**`subscribe`, `history`** for *Message history*. My inference that a REST `GET
/messages` needs only `history` (because it is a history query, not a live
subscription) is *plausible but unverified*, and it **contradicts the strongest
artifact in this memo**. Until confirmed empirically, treat `subscribe` as
potentially required for REST reads and include it in read tokens by default (see
must-verify list). **Confidence: high on the doc table itself; medium on the
per-REST-endpoint mapping** (Ably documents this against the realtime SDK, not per
REST route).

### Sub-question 4 — Token lifecycle & SDK auth patterns

**Client auth options (verbatim, `ably-js` `ClientOptions`):** `key` (Basic, and lets
the SDK self-sign TokenRequests); `token` / `tokenDetails` (a pre-obtained credential —
**no auto-renewal**, "mostly useful for testing"); `authUrl` (+ `authHeaders`,
`authParams`, `authMethod`) — the SDK fetches a token from your endpoint; `authCallback`
— your function returns an Ably Token string, a signed TokenRequest, a TokenDetails, or
an Ably JWT; plus `useTokenAuth`, `defaultTokenParams`, `queryTime`. Only `authUrl` /
`authCallback` enable automatic renewal.

**Renewal flow (corrected, confirmed against `ably-js` `isTokenErr` + spec RSA4b):**
renewal fires on an HTTP **401** whose error code is in the range
**`40140 ≤ code < 40150`** — *not* only `40140`. On such an error, with a renewal means
configured, the client makes a **single** attempt to re-mint and resend. `40140` is the
generic base token error; `40142` is specifically "expired." Reactive single-retry is
**mandatory** (RSA4b); proactive renew-ahead using `expires` and a persisted
server-time offset is **optional** (RSA4b1 / RSA10k). No renewal means → `40171`;
callback/authUrl failure → `40170`.

**`TokenParams`:** `capability` (JSON string; omitted → key's caps), `clientId`, `ttl`
(default 60 min), `timestamp` (anti-replay, within a ~2-minute server window), `nonce`
(auto, ≥16 random chars). `queryTime` fetches Ably server time for accurate TokenRequest
timestamps on clock-skewed hosts; the offset is persisted and reused.

**Canonical architecture:** app server holds the key and mints tokens; untrusted
clients use token auth via `authUrl`/`authCallback`. Two server token styles: **Ably
JWT** (no SDK, serverless-friendly) vs **native Ably Token** via `/requestToken`
(better for large/confidential capability lists).

**Rust modeling** (closest analog is `ably-go`, whose `AuthCallback func(ctx,
TokenParams) (Tokener, error)` returns a sum type): a Rust `TokenSource` enum over
`TokenString | TokenRequest | TokenDetails | AblyJwt`, an async provider trait, a
thread-safe cache (`Arc<RwLock<AuthState>>` holding token + `expires` + clock offset),
and a **single-flight** guard around renewal so concurrent 401s don't stampede.
**Confidence: high.**

### Sub-question 5 — Rust prior art & building blocks

Covered in *Background* (the `ably` crate is the sole prior art). Building blocks for a
fresh implementation:

- **TokenRequest signing:** `hmac` + `sha2` + `base64` (0.22 Engine API — this crate
  already uses it; note `ably-rust` pins the deprecated `base64 0.13`), `serde_json`,
  and a random nonce. **The canonicalization is the trap** (Finding 2).
- **Ably JWT:** `jsonwebtoken` — `Header::new(HS256)`, `header.kid = Some(keyName)`,
  serde-renamed `x-ably-*` claims, `EncodingKey::from_secret(secret)`.
- **Async provider:** `#[async_trait]` + `Arc<dyn TokenProvider + Send + Sync>` (or the
  manual boxed-future idiom `ably-rust` uses); AWS's `ProvideCredentials` is the
  reference design for the caching/refresh decorator.

### Sub-question 6 — The contrarian / boundary view (Devil's Advocate)

See the dedicated **Devil's Advocate** section below.

---

## Three findings that shape the design

### Finding 1 — Ably itself draws the boundary at the core client, not the product

Verified verbatim (`chat/setup.mdx`): *"Authentication is configured on the Ably
Pub/Sub client, which the Chat client wraps. The Chat SDK itself doesn't handle
authentication directly — it uses the authenticated connection from the underlying
Pub/Sub client."* Token issuance is a platform operation (`/keys/{keyName}/requestToken`
on `rest.ably.io`), and `Auth` is a property of the *core* REST/Realtime client,
structurally separate from any product's resource collection. This is the strongest
single argument for keeping platform-auth machinery **out** of a `/chat/v4`-scoped
crate, and it directly supports ADR-0001/0005.

### Finding 2 — `c14n` is required for TokenRequests, irrelevant for JWTs

The capability string signed into a TokenRequest MUST be canonicalized (`ably-js`
`c14n`: `keys.sort(); ops.sort(); JSON.stringify(...)` — ascending lexicographic keys
*and* ops, no whitespace). Get it wrong and the server-side MAC check fails **silently**
(opaque auth rejection). But for a **JWT**, the signature covers the entire payload, so
the server parses whatever JSON you send — **no canonicalization needed**. This is why
the JWT route is both safer and simpler, and why (if the crate ships any minter) it
should be the JWT minter, not TokenRequest signing.

Rust caveat: a `BTreeSet<Op>` over an operation enum sorts by *discriminant*
(declaration order), which is wrong. Use `BTreeSet<String>` or sort by the serialized
string. `serde_json` compact output matches `JSON.stringify`; JS UTF-16 vs Rust UTF-8
sort order diverges only above the BMP (irrelevant for ASCII operation names).

### Finding 3 — The room-resource question (RESOLVED in iteration 2; earlier interim advice was wrong)

The Chat channel is `roomName::$chat`, but Ably's own auth doc scopes capabilities using
the **bare** room name (`'your-room': ['publish', …]`). Under the generic channel-matching
rules a bare `your-room` matches only the literal channel `your-room`, not `your-room::$chat`
— which is why this looked like a trap. **Iteration-2 research resolved it** (see Addendum
§A2.2): Ably runs a **product-qualifier model** for Chat. A chat room has **two** capability
surfaces — the realtime channel `room::$chat` *and* the REST API `/chat/v4/rooms/{room}/…`
— and a single room-scoped resource is meant to authorize **both**. Ably's own SDK states it
verbatim (`ably-chat-js` `UPGRADING.md`): *"The `[chat]` qualifier now represents both the
REST and Channel access for the new single `my-room::$chat` channel,"* via
`capability: { '[chat]my-room': ['PUBLISH'] }`.

**Correction (this reverses the earlier interim advice):** scope by **room name** — bare
`"{room}"` (Ably's documented form, relies on server-side product expansion) or the explicit
qualifier `"[chat]{room}"`. **Do NOT scope to `"{room}::$chat"`** — that authorizes only the
*realtime channel* and, because this crate is REST-only, would very likely `40160` on the
`/chat/v4` REST calls (send/history/occupancy/edits). The earlier "`::$chat` is the safe
least-privilege choice" was backwards for a REST client. Confidence: **moderate-high** (the
`[chat]` mechanism is documented only in a 0.7-era upgrade note; the SDK itself never mints
tokens). A live-app **test matrix** is still recommended before shipping `for_room`: mint a
token with each of `[chat]{room}`, bare `{room}`, `{room}::$chat`, `{room}:*` and confirm
send + history + occupancy succeed without `40160`.

---

## Recommendation — a tiered design for `ably-chat-rs`

The design principle: **stay a Chat-REST client; own the *safe, high-value, greenfield*
pieces; delegate the platform-auth machinery.** Everything below is additive and
backward-compatible with the current `enum Auth`.

**A boundary caveat this recommendation must own.** Finding 1 (Ably puts auth on the
*core* client, not the product layer) is the reasoning used below to decline Tier 4 —
and in fairness it cuts against Tiers 1–2 as well: typed capabilities and a JWT minter
are generic *platform-auth* utilities, not chat-specific concerns, so their principled
long-term home is a shared auth crate or upstream `ably`, not a `/chat/v4` client. The
justification for building them *here anyway* is pragmatic, not architectural: **no Rust
crate offers typed capabilities or Ably-JWT minting today**, and this crate's server-side
callers need them now. That is a defensible reason to ship them behind feature flags as a
stopgap — but the ADR should record "**long-term home may be a shared/upstream auth
crate**" as an explicit open question, and Tiers 1–2 should be designed so they could be
extracted (or replaced by an upstream dependency) without breaking `ably-chat-rs`'s public
surface.

### Tier 0 — Keep what exists
`Auth::ApiKey` (Basic) and `Auth::Token` (Bearer). No change. This already covers the
dominant server-side case, where the key's dashboard-configured capability is enforced
server-side and no minting is needed.

### Tier 1 — Typed capability model  *(feature = "capabilities"; recommended, low risk)*
No Rust crate offers this; it mints nothing, so there is zero secret-handling risk, and
it removes a whole class of stringly-typed errors.

```rust
/// An Ably capability operation (the 17 platform operations).
#[non_exhaustive]
pub enum Operation {
    Subscribe, Publish, Presence,
    ObjectSubscribe, ObjectPublish,
    AnnotationSubscribe, AnnotationPublish,
    MessageUpdateOwn, MessageUpdateAny,
    MessageDeleteOwn, MessageDeleteAny,
    History, Stats, PushSubscribe, PushAdmin,
    ChannelMetadata, PrivilegedHeaders,
}

/// A capability document: resource pattern -> allowed operations.
/// Serializes to canonical Ably form (sorted keys, sorted ops, no whitespace).
pub struct Capability(BTreeMap<String, BTreeSet<String>>); // sort by serialized op string

impl Capability {
    pub fn allow(self, resource: impl Into<String>, ops: impl IntoIterator<Item = Operation>) -> Self { /* … */ }
    /// Scope to a chat room. Targets the room resource Ably uses for chat — bare
    /// `"{room}"` (or the explicit product qualifier `"[chat]{room}"`) — which
    /// authorizes BOTH the `/chat/v4` REST API and the `::$chat` channel. Do NOT
    /// scope to `"{room}::$chat"` alone: that covers only the realtime channel and
    /// would 40160 on the REST calls this crate makes (Finding 3 / Addendum §A2.2).
    pub fn for_room(self, room: &str, ops: impl IntoIterator<Item = Operation>) -> Self { /* … */ }
    pub fn to_json(&self) -> String { /* canonical */ }
}
```

Ship the chat-relevant operations prominently; keep the full 17 for completeness. Model
resource patterns as strings (wildcards are just strings) but provide `for_room` so
callers don't hand-roll the `::$chat` suffix. Document the intersection semantics.

### Tier 2 — Ably JWT minter  *(feature = "jwt"; recommended, moderate risk, server-side only)*
The "safe half" the contrarian view concedes and Ably's recommended pattern for issuing
end-user credentials. No network, no canonicalization (Finding 2), just `jsonwebtoken`.

```rust
/// SERVER-SIDE ONLY. Requires your API secret; never call this in client-shipped code.
pub fn mint_ably_jwt(api_key: &ApiSecret, params: &TokenParams) -> Result<String>;

pub struct TokenParams {
    pub capability: Capability,        // -> x-ably-capability (JSON string)
    pub client_id: Option<String>,     // -> x-ably-clientId
    pub ttl: Duration,                 // -> exp = iat + ttl
}
```

Guard rails: (1) a `struct ApiSecret(String)` newtype with a redacting `Debug`, so the
type system signals "this is secret"; (2) a feature flag (`jwt`) so it isn't in the
default build; (3) prominent "server-side only" docs; (4) enforce/clamp `ttl` to Ably's
maxima and surface `40003` semantics. Consider a sibling `#[cfg]`-gated warning in the
module docs.

### Tier 3 — `Auth::Provider` refresh seam  *(the reserved ADR-0005 extension; moderate complexity)*
This is the piece the current code explicitly anticipates. It requires turning the
**static** `Inner.auth_header: String` into a **dynamic** value and adding a 401-driven
re-auth path — neither exists today (`dispatch.rs` only retries on 429/5xx/timeout).

```rust
#[async_trait]
pub trait TokenProvider: Send + Sync {
    /// Return a currently-valid Bearer credential (Ably Token string or Ably JWT).
    async fn token(&self) -> Result<String>;
}

pub enum Auth {
    ApiKey(String),
    Token(String),
    Provider(Arc<dyn TokenProvider>),   // NEW
}
```

Dispatch changes:
- Resolve the `Authorization` header per request. For `Provider`, read a cached token
  (guarded by `Arc<RwLock<CachedToken>>`), refreshing when near `expires` (proactive,
  optional) — with a **single-flight** guard so concurrent requests don't stampede.
- On an HTTP **401** whose Ably error code is in `40140..40150`, invoke the provider
  once, then retry the request a single time (spec RSA4b). Do not loop.
- Keep the crate out of the platform business: the *provider* is where the caller (or
  the `ably` core crate, or the caller's own HTTP call to their auth server) obtains the
  token. `ably-chat-rs` never calls `requestToken` itself.

Because this adds a variant to a currently-exhaustive public enum, sequence it with the
`#[non_exhaustive]` decision noted in `config.rs` (the TODO explicitly flags this).

### Tier 4 — DECLINE: TokenRequest signing + `requestToken` exchange
Do **not** implement HMAC TokenRequest signing or the `POST /keys/{keyName}/requestToken`
exchange in this crate. Rationale: it is a *platform* endpoint outside `/chat/v4`
(Finding 1); it is the fiddliest, silently-failing piece (Finding 2's canonicalization,
the 2-minute timestamp window, clock-offset logic); and the official `ably` crate already
implements it. Instead, document an **interop recipe**: use `ably::Auth::create_token_request`
/ `request_token` (or your own auth server) to obtain a token, then pass it to
`ably-chat-rs` via `Auth::Token` or `Auth::Provider`. If demand is strong, revisit as a
thin optional dependency on the `ably` crate rather than a reimplementation.

### Feature-flag / crate-shape summary

| Piece | Where | Feature | Network? | Secret? | Risk |
|---|---|---|---|---|---|
| Basic / Token static | core (exists) | — | no | key only for Basic | none |
| Capability types | new module | `capabilities` | no | no | low |
| Ably JWT minter | new module | `jwt` | no | **yes** (secret) | moderate (mitigated) |
| `Auth::Provider` refresh | client + dispatch | default or `refresh` | via provider | no | moderate |
| TokenRequest signing / exchange | — | — | yes | yes | **declined** |

---

## Contradictions & disputed claims

1. **`requestToken` host** — docs UI shows `main.realtime.ably.net`; the machine spec
   and this repo use `rest.ably.io`. **Resolved:** `rest.ably.io` (spec-authoritative).
2. **"40140 = expired"** (a common shorthand, incl. the original task framing) —
   **corrected:** renewal triggers on the *range* `40140..40150`; `40142` is "expired,"
   `40140` is the generic base error.
3. **Operation count** — `token-request-spec.mdx` lists 13 (omits the four
   `message-*-own/any`); `capabilities.mdx` lists 17. **Resolved:** 17 is authoritative;
   the spec page is stale.
4. **Room capability resource key** — **resolved in iteration 2** (§A2.2): scope by room
   name (bare `"{room}"` / `"[chat]{room}"`), not `"{room}::$chat"`; live confirmation
   still recommended.
5. **`capability` vs `capabilities`** in the signed string — the singular `capability`
   value; docs prose is loose. **Resolved** via code.

---

## Devil's Advocate (steelman: don't build it here)

Ably's own product architecture answers the boundary question, and the answer is "not in
the chat layer": the Chat SDK wraps a pre-authenticated core client and *"doesn't handle
authentication directly."* Token issuance is a platform operation; `Auth` is a property
of the core client. Putting capability minting inside a `/chat/v4` crate places
platform-auth machinery behind a product-scoped API — the exact coupling Ably's layering
avoids — and contradicts ADR-0001/0005.

Making minting *easy* in a client crate is a security anti-pattern, because signing
requires the API **secret**. Ably is explicit that the secret must stay on "trusted
secure servers" and "never be exposed in client-side code," and OWASP is categorical that
any secret shipped in a distributed binary is compromised by definition. A convenient
`create_token_request()` invites developers to call it wherever handy — including shipped
clients.

For the crate's dominant use case, minting is redundant: a Rust REST client for
`/chat/v4/*` is the server-side case, authenticates with the API key, and the key's
configured capabilities are enforced server-side. Token minting exists to hand
short-lived, narrowed credentials to *untrusted* clients — a browser/mobile/realtime
concern this crate's users largely don't have.

And signing is fiddly and fails silently: the canonical string demands an exact field
order with a newline after every field including an empty `clientId` and trailing
`nonce`, over a canonicalized capability JSON; any deviation yields not a clear error but
an opaque auth rejection in production. Ably itself says JWTs need no SDK at all — so the
ecosystem already has the right home (a generic `jsonwebtoken` helper), and TokenRequest
signing already has one (the official `ably` crate).

**Where this argument yields.** It targets *TokenRequest signing* and *stateful refresh*,
not *capability types* or *JWT minting*. Typed capabilities mint nothing (zero secret
risk) and genuinely help even a server-side caller construct correct tokens/keys. Ably
JWTs are just HS256 signable with any library and no live call — a thin, feature-gated,
"server-side only" JWT helper matches Ably's *recommended* pattern. So the sharpest
build-it position is exactly the tiered recommendation above: ship capability types + a
JWT minter behind flags, land the refresh seam, and decline TokenRequest signing /
`requestToken` in favor of the `ably` crate. That carve-out neutralizes the security and
correctness objections while respecting the boundary.

---

## Confidence & gaps

**Overall confidence: high** on the mechanics; **medium** on two chat-specific details.

Per sub-question: Auth mechanisms — **high** (SDK code + machine spec). Capability model
— **high** (verbatim authoritative doc). Chat mapping — **high** on the feature table and
channel name (source-confirmed); **medium** on the per-REST-endpoint operation and on
reaction-*read* gating. Lifecycle — **high** (features spec, corrected). Rust prior art —
**high** (source-confirmed). Boundary — **high**.

**Strongest evidence:** the TokenRequest signing string and `c14n` (dual-sourced: code +
spec); the 17 operations and the feature→capability table (verbatim); the renewal range
(from `isTokenErr` + RSA4b); the delegation quote (verified line).

**Weakest / open:**
- **Room-resource matching (Finding 3)** — resolved to bare/`[chat]` room name (§A2.2),
  moderate-high; a live test-matrix confirmation is still worth doing before shipping Tier 1.
- **`subscribe` for REST reads** — resolved (§A2.3): REST reads need only `history`, not
  `subscribe` (the occupancy precedent confirms the table's `subscribe` is a realtime-attach
  artifact). Strong inference, not verbatim per-endpoint — a `40160` probe would confirm.
- **Reaction-read capability** — `GET …/client-reactions` returns a *summary* (most
  defensibly gated by `subscribe`/`history`); `annotation-subscribe` gates raw individual
  annotations. Undocumented at endpoint granularity.
- **REST enforcement of `-own`/`-any`** — well-founded inference (capabilities are
  platform-level and transport-agnostic), not an explicitly documented fact for the REST
  route.
- **JWT size limit** — Ably advises native tokens over JWTs when the capability JSON is
  large/confidential; exact threshold not pinned.

**Recommended follow-up:**
1. Empirically test bare-room-name vs `room:*` vs `room::$chat` capability scoping
   against a live Ably app (resolves Finding 3).
2. Confirm the per-endpoint required operations by probing `40160` responses — in
   particular whether REST `GET /messages` (history/read) requires `subscribe` in
   addition to `history`, plus the reaction-read and occupancy endpoints.
3. Decide the `#[non_exhaustive]` timing for `enum Auth` before adding `Provider`.
4. Prototype Tier 1 + Tier 2 behind flags; defer Tier 3 to its own ADR (it touches the
   dispatch hot path).

---

## Sources

Primary (code / machine spec):
- `ably/ably-js` — `src/common/lib/client/auth.ts` (signing `signText`, `c14n`,
  `isTokenErr`).
- `ably/ably-chat-js` — `src/core/channel.ts` (`roomChannelName`), `src/core/chat-api.ts`
  (REST endpoints).
- `ably/ably-rust` — `src/auth.rs`, `Cargo.toml` (the sole Rust prior art).
- `ably/ably-go` — `ably/options.go` (auth option modeling).
- `ably/open-specs` — `definitions/platform-v1.yaml` (host, `requestToken`, security schemes).
- Ably client-library features spec — RSA4/RSA9/RSA10 (renewal, TokenParams, server time).

Primary (docs, `ably/docs` main branch, read as raw markdown):
- `auth/capabilities.mdx` (17 operations, wildcards, intersection, JSON format).
- `auth/identified-clients.mdx` (clientId semantics).
- `chat/authentication.mdx` (feature→capability table, JWT example).
- `chat/setup.mdx` (auth-delegation quote), `chat/rooms` (single-channel backing).
- `api/token-request-spec.mdx` (canonical signing string), `auth/token/jwt` (JWT format).
- `platform/errors/codes` + `ably-common` `errors.json` (error codes).

Secondary / supporting: OWASP mobile secrets guidance (client-side-signing risk); the
rendered Ably doc site (corroboration only).

This repo (primary artifacts): `README.md`, `openapi/ably-chat-rest.yaml`,
`crates/ably-chat-openapi/src/apis/configuration.rs`, `crates/ably-chat-rs/src/{config,client,dispatch}.rs`,
`docs/adr/0001-rest-only-scope.md`, `docs/adr/0005-authentication.md`.

---

## Appendix A — the crate today (grounding)

- `config.rs`: `enum Auth { ApiKey(String), Token(String) }`; `header_value()` renders
  `Basic base64(key)` or `Bearer <token>`; `Debug` redacts. TODO reserves
  `Auth::Provider(Arc<dyn TokenProvider>)`; `#[non_exhaustive]` intentionally *not* yet
  added.
- `client.rs`: `Inner` holds a **precomputed static** `auth_header: String`. `Debug`
  redacts credentials.
- `dispatch.rs`: applies `X-Ably-Version: 4` + the static `AUTHORIZATION` header on every
  attempt; retries only on 429/5xx/timeout — **never 401**. This is the seam Tier 3 must
  change (static string → provider-resolved value + a 401/`40140..40150`→re-auth→single-retry path).
- ADR-0001: REST-only. ADR-0005: static credentials only in 0.x; auto-refresh deferred.

## Appendix B — worked capability examples

Self-service REST client (own messages) for room `sports`:
```json
{ "sports": ["publish", "history", "message-update-own", "message-delete-own", "annotation-publish"] }
```
REST moderation client (any message) across all rooms:
```json
{ "*": ["history", "message-update-any", "message-delete-any"] }
```
Caveats (updated in iteration 2): (1) scope by **room name** — bare `"sports"` (Ably's
documented form) or the explicit qualifier `"[chat]sports"`; do **not** use
`"sports::$chat"`, which covers only the realtime channel and would `40160` on the
`/chat/v4` REST calls (Finding 3 / §A2.2). (2) REST reads need only `history`, not
`subscribe` (§A2.3). (3) add `channel-metadata` for occupancy. (4) verify with the live
test matrix in §A2.2.

---

# Addendum — iteration 2 (2026-07-24): management workflow, resolved gaps, interop

This addendum folds in a second research round scoped to the open questions the body
flagged, plus the *management* angle. Where it differs from the body above, **this is
newer**. It leads with the permission-management playbook (the operational question),
then records the resolutions.

## A2.1 — Permission-management playbook (the operational model)

**Core frame (confirmed):** Ably has **no built-in role or ACL system** for Chat.
"Permissions" are **capabilities baked into a token (or API key)**, enforced server-side.
Roles, muting, banning, and "membership" are all **application-level patterns** assembled
from scoped tokens, the `-own`/`-any` split, `clientId` identity, and token revocation.
There is no server-side "who-is-in-this-room / what-is-their-role" store to toggle.

**Changing a user's permissions at runtime — two levers, both routed through *your* auth
server:**
1. **Re-issue + `authorize()`** — your auth server begins issuing a token with different
   capabilities; the client adopts it at natural refresh (≤ TTL, default 1 h) or
   immediately if you signal it to call `auth#authorize()` (no connection disruption).
   *"There's no special mechanism for this; you can use whatever mechanism you normally
   use to communicate with the client to tell it to call `authorize()`."*
2. **Revoke** — force the client back to your `authCallback` now instead of at TTL.

**The subtlety that trips people up:** revocation *alone* changes nothing — it just forces
re-auth; if your issuing policy is unchanged, the **same** capabilities come straight back.
**Policy is the permission; `authorize()`/revoke only control *when* it is picked up.**
Latency: no action → up to the full TTL (default/max 1 h); `authorize()` → near-instant but
needs client cooperation (no server push); revoke → near-immediate (or +30 s with a reauth
margin).

**Role tiers as concrete capability JSON** (resource key per §A2.2 — bare room name):

```json
// lurker (read-only)
{ "my-room": ["history", "annotation-subscribe", "channel-metadata"] }
// participant (normal member; manages own messages)
{ "my-room": ["publish", "history", "presence", "channel-metadata",
              "annotation-publish", "annotation-subscribe",
              "message-update-own", "message-delete-own"] }
// moderator (participant + acts on ANY message — note -any, not -own)
{ "my-room": ["publish", "history", "presence", "channel-metadata",
              "annotation-publish", "annotation-subscribe",
              "message-update-any", "message-delete-any"] }
```
(Realtime consumers would add `subscribe`/`presence`; a REST-only client does not — §A2.3.
Widen the resource key to `dms:*` or `*` for multi-room/admin tokens. Do **not** copy
Ably's "all Chat features" list literally — it uses `-own`; a moderator needs `-any`.)

**Moderation.**
- *Acting on another user's message* needs `message-update-any` / `message-delete-any`;
  `-own` only matches messages whose publisher `clientId` equals the caller's. A moderator
  can do this over REST. **Endpoint note:** this crate's Chat REST surface uses
  `POST /chat/v4/rooms/{room}/messages/{serial}/delete` (soft delete) and
  `PUT …/{serial}` (edit) — ground-truthed from `ably-chat-js`. Ably's *generic platform*
  message API expresses the same as a single `PATCH /channels/{channelId}/messages/{serial}`
  with a `MessageAction` enum body — a **different API surface**; the crate speaks the
  `/chat/v4` verbs, so there is no conflict, just don't confuse the two.
- *Content moderation* is a separate axis (not capabilities): **before-publish** (an
  integration rule invokes a webhook / direct connector — Hive, Tisane, Azure Content
  Safety — in the publish path; returns `accept` / `reject` / `modify`) vs **after-publish**
  (no chat-specific API — use standard integration rules to forward the message to your
  infra, then delete it via REST). Before-publish adds latency but nothing bad appears;
  after-publish is async but briefly visible.

**Mute / ban — there is no mute/ban API.** Each is **two coordinated actions**: (1) change
the auth-server policy for that `clientId` — drop `publish` (mute) or drop the room resource
entirely (ban); (2) **revoke** the current token to force immediate pickup. You can mute in a
single room by removing `publish` from just that room's resource in the next token.

**Room membership:** none server-side. *"Ably Chat currently does not offer a room membership
feature where clients belong to rooms long-term."* Membership is only **presence** (who is
connected now) + **occupancy** (counts). A durable roster/allow-list is **the app's to own**
and translate into capabilities at token-issue time.

**Identity:** `clientId` is what makes this trustworthy — it is server-assigned and
non-spoofable (*"No other clients are permitted to assume a `clientId` that they are not
assigned"*), which is exactly what makes `-own` meaningful. The wildcard `clientId: "*"`
(assume any identity) is for **server/admin actors only** — never issue it to end users.

**Chat SDK surface:** none for permissions. `RoomOptions` toggles *features* (presence,
occupancy, typing, reactions), not authorization; `messages.update/delete` take only
audit metadata (`description`, `metadata`), no role parameter. Success is decided purely by
the token's capabilities on the room.

**Ably-native vs. app-built**

| Ably-native (platform enforces) | App must build |
|---|---|
| Operations + `-own`/`-any` split; resource scoping; `clientId` anti-masquerade; token revocation; REST edit/delete; before-publish integration + connectors; presence/occupancy | The *concept* of roles and their mapping to capability JSON; an auth server that issues scoped tokens; a durable membership/allow-list store; the mute/ban *policy*; filtering soft-deleted messages; the after-publish moderation loop |

## A2.2 — RESOLVED: the room-resource question (was Finding 3)

Covered inline in **Finding 3** above (corrected). Summary: scope by **room name** — bare
`"{room}"` or the explicit product qualifier `"[chat]{room}"` — which authorize **both** the
`/chat/v4` REST API and the `room::$chat` channel. **Not** `"{room}::$chat"` (channel-only →
`40160` on REST). Evidence: `ably-chat-js` `UPGRADING.md` (`[chat]` = "both REST and Channel
access"). Confidence moderate-high; still merits the live test matrix
(`[chat]{room}` · bare `{room}` · `{room}::$chat` · `{room}:*`).

## A2.3 — RESOLVED: per-endpoint REST capability map

REST reads need only **`history`** — **not** `subscribe`. The `subscribe` in Ably's
feature→capability table is a realtime-attach artifact; the clinching precedent is
*occupancy*, where the same table lists `subscribe, channel-metadata` yet Ably's REST
reference states the metadata endpoint needs only `channel-metadata`. Full map:

| Endpoint | Operation | Verdict |
|---|---|---|
| `POST /messages` | `publish` | documented |
| `GET /messages`, `/{serial}`, `/versions` | `history` (only) | strong inference |
| `PUT /messages/{serial}` | `message-update-own` \| `-any` | documented map; REST enforcement inferred |
| `POST /messages/{serial}/delete` | `message-delete-own` \| `-any` | documented map; REST enforcement inferred |
| `POST\|DELETE /messages/{serial}/reactions` | `annotation-publish` | documented |
| `GET /messages/{serial}/client-reactions` | `subscribe` **or** `history` (summary read; **not** `annotation-subscribe`) | strong inference |
| `GET /occupancy` | `channel-metadata` (only) | documented |

So a REST **read** token needs `history` (+ `channel-metadata` for occupancy); a REST
**write** token adds `publish`, `message-*-*`, `annotation-publish`. `subscribe`/`presence`
are realtime-only. (Highest-value live probe: a `history`-but-not-`subscribe` token → `GET
/messages` should succeed.)

## A2.4 — Revocation & the correct `TokenProvider`

- **`POST /keys/{keyName}/revokeTokens`** — on the **data-plane** host (`rest.ably.io`),
  **basic auth**, key-holder only (a token-auth client calling it gets `40162`). So it is a
  **server-side admin** call — keep it out of the client `TokenProvider`. Requires the key
  attribute **`revocableTokens = true`** (set *before* the token was issued).
- **Targets** (array, ≤100): `clientId:…`, `channel:…`, or `revocationKey:…`; plus
  `issuedBefore` (revoke all issued before T; default now) and `allowReauthMargin` (true →
  enforce at +30 s and hint live connections to upgrade). Response `appliesAt` = effective
  time. No separate propagation delay is documented.
- **JWTs are revocable** via a `revocationKey:` target + an **`x-ably-revocation-key`** claim
  in the JWT (Ably Tokens revoke by `clientId`/`channel`). → the Tier-2 JWT minter should
  optionally emit this claim.
- **Revocable tokens are capped at 1 h TTL** (vs 24 h standard).
- **Refresh interaction:** the auto-renew range `40140 ≤ code < 40150` covers *both*
  `40141` (revoked) and `40142` (expired); make **exactly one** re-mint + retry, never loop.
  A *fresh* token isn't revoked, so retry normally succeeds — "don't re-mint a revoked
  identity" is an **auth-server policy** decision, not something the spec enforces.
- **Clock/timestamp** (2-minute window, nonce, `queryTime` server-time-offset caching) apply
  **only to local TokenRequest signing** — irrelevant to JWTs and to `authUrl`/`authCallback`
  tokens.

**`TokenProvider` checklist:** single retry on `[40140,40150)`; fail fast with `40171` if no
renewal means; distinguish `40142` (routine) from `40141` (security event → log/backoff, and
don't tight-loop); proactive refresh margin (~15–30 s before `expires`) vs server-adjusted
time; respect the TTL ceiling; keep `revokeTokens` server-side.

## A2.5 — Control API: the capability model's second consumer

The Ably **Control API** (`control.ably.net/v1`, authed by an *account* access token, not
the app key) manages apps/keys/namespaces. `POST /apps/{app_id}/keys` takes `capability` as
the **same `{resource: [ops]}` map** — but as a **native JSON object**, whereas tokens/JWTs
need the **canonicalized JSON string**. So the typed `Capability` model has a **real second
consumer**: share one value type, provide **two encoders** (object for Control-API key
create/update; canonicalized string for TokenRequest/JWT), keep envelopes separate. No Rust
Control-API client exists (Go + Terraform only). This strengthens Tier 1. (Aside: `$chat` is
a platform-reserved qualifier that auto-applies chat channel defaults — persistence, 30-day
retention.)

## A2.6 — Interop with the `ably` crate + competitor comparison

**Verified interop recipe** (server mints a scoped token, hands it to `ably-chat-rs`):
```rust
use ably::auth::{AuthOptions, Credential, Key, TokenParams};
let rest = ably::Rest::from("appId.keyId:secret");
let params = TokenParams::new()
    .client_id("user-123")
    .capability(r#"{"my-room":["publish","history"]}"#)   // bare room name (§A2.2), NOT ::$chat
    .ttl(chrono::Duration::minutes(60));
let opts = AuthOptions { token: Some(Credential::Key(Key::try_from("appId.keyId:secret")?)),
                         ..Default::default() };           // signing key comes from opts.token
let details = rest.auth().request_token(&params, &opts).await?;
let chat = ably_chat::Client::builder(ably_chat::Auth::token(details.token)).build();
```
**Do not take an optional dependency on `ably`** today: version skew is disqualifying — it
pins `reqwest 0.11` + `base64 0.13` vs this crate's `reqwest 0.13` + `base64 0.22` (would
duplicate the HTTP/TLS stack) and drags in crypto/msgpack + a native-tls default. Keep `ably`
a **documented interop target** (confirms Tier 4). Residual gaps `ably` does *not* fill:
typed capabilities, Ably-JWT minting, stateful refresh — all greenfield.

**Competitor check (the model is conventional):**

| Provider | Permission model | Server token minting | Rust SDK |
|---|---|---|---|
| **Ably** | capability map `{resource → [ops]}`, wildcards, key∩token | Ably JWT (HS256) or TokenRequest→`/requestToken` | `ably` (REST preview); no typed caps / JWT mint |
| **PubNub** | grant token: per-resource flags (read/write/manage/…), list or regex, bound uuid, TTL | `grant_token()` with secret key | **Official Rust** `grant_token()` |
| **Pusher** | coarse: channel-*type* gated (`private-`/`presence-`) | auth endpoint signs `HMAC(socket_id:channel)` | community `pusher` crate |
| **Stream** | role-based (server-config); token = identity JWT | JWT (HS256) with `user_id` | none official |

PubNub is a near-identical analog with an **official Rust `grant_token()`** — direct
precedent for both Ably's capability-map approach and the proposed
`Capability`/`TokenParams`/`mint_ably_jwt` ergonomics. The only genuinely novel-in-Rust piece
is typed Ably capabilities + Ably-JWT minting (exactly Tier 1/Tier 2).

## A2.7 — Recommendation deltas (supersede the body where they differ)

- **Tier 1 `for_room`:** default to bare `"{room}"`; offer `"[chat]{room}"`; never
  `"{room}::$chat"`. Provide **two serializers** (canonical string for token/JWT; JSON object
  for Control API — §A2.5). Consider shipping **role presets** (lurker/participant/moderator)
  as ergonomic constructors, clearly labeled app-level conventions (§A2.1).
- **Tier 2 JWT minter:** optionally emit the `x-ably-revocation-key` claim so issued JWTs are
  revocable (§A2.4).
- **Tier 3 `Auth::Provider`:** implement the §A2.4 checklist — single retry on `[40140,40150)`,
  distinguish revoked vs expired, don't tight-loop, proactive margin, keep `revokeTokens`
  server-side.
- **Tier 4:** interop recipe verified; still decline building TokenRequest signing / the
  `requestToken` exchange, and don't depend on `ably` yet (§A2.6).

## A2.8 — Updated confidence & gaps

- **Resolved:** `subscribe`-for-reads (→ `history` only); Control-API as a second capability
  consumer; the interop path and the don't-depend decision; the room-resource scoping (bare /
  `[chat]` room name, moderate-high).
- **Still needs a live Ably app:** the exact minimal resource authorizing both surfaces (test
  matrix in §A2.2); whether `-own`/`-any` is enforced on the REST edit/delete route; the
  reaction-summary read gate; revocation latency under load. None blocks a *prototype* — all
  are confirmable with a handful of `40160`/`40141` probes against a real app.
