#![cfg(all(feature = "capabilities", feature = "token-issuance"))]
//! Live probe for the resource form that `Capability::for_room` emits
//! (ADR-0012 "Open verification", SPEC §13.1, research memo §A2.2).
//!
//! `for_room(room, ops)` scopes a token by the **bare room name** on the claim that
//! Ably's product model expands it to authorize both the `/chat/v4` REST calls this
//! crate makes and the underlying `room::$chat` channel. That claim rests on a
//! 0.7-era `[chat]`-qualifier note and is documented only to moderate-high
//! confidence; it gates pre-1.0 stabilization of `for_room`. This test converts the
//! open question into a one-command job: it mints a token per candidate resource
//! pattern and prints a send/history/occupancy verdict matrix.
//!
//! **Scope limit:** this settles only the REST half of the claim. Being a REST-only
//! crate (ADR-0001), it cannot observe whether the same resource also authorizes the
//! `room::$chat` realtime channel; that half stays inference.
//!
//! **Requires real Ably credentials** and is therefore `#[ignore]`d. Run it with:
//!
//! ```text
//! ABLY_API_KEY=appId.keyId:keySecret \
//!   cargo test -p ably-chat-rs --features token-issuance,capabilities \
//!   --test live_capability_scoping -- --ignored --nocapture
//! ```
//!
//! **Cost against a real app:** up to two token mints per pattern (one eager
//! diagnostic mint, plus the one the `Client`'s provider makes on its first
//! request) — so ~8 mints for the four patterns — and up to four published chat
//! messages, in a room named `cap-probe-{unix-seconds}`. Chat rooms are implicit
//! server-side, so nothing is created or deleted and no cleanup is needed. The
//! room name is unique per run so concurrent or repeated runs do not collide.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ably_chat::prelude::*;
use ably_chat::{Capability, KeyTokenProvider, Operation, TokenProvider};

/// Ably's "operation not permitted" / capability-denied error code.
const CAPABILITY_DENIED: i64 = 40160;

/// The `clientId` bound into every issued token. Chat message sends carry no
/// `clientId` in the body — the publisher identity comes from the token — so
/// binding one keeps a `send` failure a statement about *capability* rather than
/// about identity.
const PROBE_CLIENT_ID: &str = "cap-probe";

/// The operations every candidate pattern is granted: `publish` (send),
/// `history` (history), `channel-metadata` (occupancy). `Operation` is `Clone`,
/// not `Copy`, so each row builds a fresh array.
fn probe_ops() -> [Operation; 3] {
    [
        Operation::Publish,
        Operation::History,
        Operation::ChannelMetadata,
    ]
}

/// The verdict for one cell of the matrix (or, as `MintFailed`, for a whole row).
#[derive(Debug)]
enum Outcome {
    /// The operation succeeded: the capability authorized it.
    Ok,
    /// Ably rejected the operation with `40160`: the capability did not cover it.
    Denied,
    /// Any other failure — *not* a capability verdict.
    Err {
        status: Option<u16>,
        code: Option<i64>,
    },
    /// The token request itself was rejected, so no probe ran for this row.
    /// Distinguishing this from `Denied` matters: a bad key, or a capability the
    /// issuing key cannot delegate, fails at mint time and would otherwise look
    /// exactly like a per-operation denial.
    MintFailed {
        status: Option<u16>,
        code: Option<i64>,
        message: String,
    },
}

impl Outcome {
    /// Classifies a probe result: `40160` is a capability denial, anything else
    /// is an unrelated error worth printing verbatim.
    fn classify<T>(result: ably_chat::Result<T>) -> Self {
        match result {
            Ok(_) => Outcome::Ok,
            Err(e) if e.info().map(|i| i.code) == Some(CAPABILITY_DENIED) => Outcome::Denied,
            Err(e) => Outcome::Err {
                status: e.status(),
                code: e.info().map(|i| i.code),
            },
        }
    }

    /// Classifies a failed token mint.
    fn mint_failed(e: &Error) -> Self {
        Outcome::MintFailed {
            status: e.status(),
            code: e.info().map(|i| i.code),
            message: e.to_string(),
        }
    }

    fn is_ok(&self) -> bool {
        matches!(self, Outcome::Ok)
    }

    fn label(&self) -> String {
        match self {
            Outcome::Ok => "OK".to_owned(),
            Outcome::Denied => format!("DENIED({CAPABILITY_DENIED})"),
            Outcome::Err { status, code } => {
                format!("ERR(status={} code={})", opt(*status), opt(*code))
            }
            Outcome::MintFailed { status, code, .. } => {
                format!("MINT_FAILED(status={} code={})", opt(*status), opt(*code))
            }
        }
    }
}

/// Renders an absent status/code as `?` rather than `None`.
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "?".to_owned(), |v| v.to_string())
}

/// One row of the matrix: a candidate resource pattern and its three verdicts.
struct Row {
    label: &'static str,
    requested: String,
    send: Outcome,
    history: Outcome,
    occupancy: Outcome,
}

impl Row {
    fn all_ok(&self) -> bool {
        self.send.is_ok() && self.history.is_ok() && self.occupancy.is_ok()
    }
}

/// Mints a token for `cap`, then runs send/history/occupancy under it.
///
/// The mint is done eagerly and separately from the `Client` so that a rejected
/// *token request* is reported as `MINT_FAILED` instead of surfacing lazily as
/// the first operation's error — where it would be indistinguishable from a
/// capability denial. On mint failure the probes are skipped and the whole row
/// carries the mint error.
async fn probe_pattern(api_key: &str, room: &str, label: &'static str, cap: &Capability) -> Row {
    let requested = cap.to_capability_string();
    let provider = KeyTokenProvider::new(api_key)
        .expect("ABLY_API_KEY must be in appId.keyId:keySecret form")
        .capability(requested.clone())
        .client_id(PROBE_CLIENT_ID)
        .ttl(Duration::from_secs(60));

    if let Err(e) = provider.token().await {
        eprintln!("[{label}] token mint failed: {e}");
        return Row {
            label,
            requested,
            send: Outcome::mint_failed(&e),
            history: Outcome::mint_failed(&e),
            occupancy: Outcome::mint_failed(&e),
        };
    }

    let client = Client::builder(Auth::provider(Arc::new(provider))).build();
    let handle = client.room(room);

    // Every probe is classified, never unwrapped: one failure must not abort the
    // run, because the other cells are the diagnostics the maintainer needs.
    let send = Outcome::classify(handle.messages().send("probe").await);
    let history = Outcome::classify(handle.messages().history().await);
    let occupancy = Outcome::classify(handle.occupancy().get().await);

    Row {
        label,
        requested,
        send,
        history,
        occupancy,
    }
}

#[tokio::test]
#[ignore = "requires a live Ably app; run with ABLY_API_KEY=... cargo test -p ably-chat-rs --features token-issuance,capabilities --test live_capability_scoping -- --ignored --nocapture"]
async fn for_room_scoping_matrix() {
    let api_key = std::env::var("ABLY_API_KEY").unwrap_or_else(|_| {
        panic!(
            "ABLY_API_KEY is not set. This test probes a real Ably app: set \
             ABLY_API_KEY to a full API key in `appId.keyId:keySecret` form \
             (copy it from the Ably dashboard) and re-run with `-- --ignored --nocapture`."
        )
    });

    let room = format!(
        "cap-probe-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_secs()
    );

    // The four candidate resource forms, in the order the research memo lists
    // them. The first row goes through the real `for_room` helper so the test
    // exercises what the crate actually emits, not a hand-written equivalent.
    let bare = Capability::new().for_room(&room, probe_ops());
    let qualified = Capability::new().allow(format!("[chat]{room}"), probe_ops());
    let channel = Capability::new().allow(format!("{room}::$chat"), probe_ops());
    let wildcard = Capability::new().allow(format!("{room}:*"), probe_ops());

    let rows = [
        probe_pattern(
            &api_key,
            &room,
            "bare room name (what for_room emits)",
            &bare,
        )
        .await,
        probe_pattern(&api_key, &room, "[chat] product qualifier", &qualified).await,
        probe_pattern(&api_key, &room, "::$chat channel suffix", &channel).await,
        probe_pattern(&api_key, &room, "wildcard under room", &wildcard).await,
    ];

    // Print the whole matrix before asserting, so the diagnostics survive a
    // failure.
    println!("\n=== for_room resource-scoping matrix ===");
    println!("room     : {room}");
    println!("clientId : {PROBE_CLIENT_ID}");
    println!("ops      : publish, history, channel-metadata\n");
    // Widths fit the longest label (`bare room name (what for_room emits)`, 36)
    // and the longest verdict (`MINT_FAILED(status=404 code=40400)`, 34).
    println!(
        "{:<38} | {:<35} | {:<35} | {:<35}",
        "PATTERN", "send", "history", "occupancy"
    );
    println!("{:-<38}-+-{:-<35}-+-{:-<35}-+-{:-<35}", "", "", "", "");
    for row in &rows {
        println!(
            "{:<38} | {:<35} | {:<35} | {:<35}",
            row.label,
            row.send.label(),
            row.history.label(),
            row.occupancy.label()
        );
        println!("    requested: {}", row.requested);
    }

    println!("\n--- INTERPRETATION ---");
    println!(
        "OK in all three columns of the `bare room name` row is the REST half of the\n\
         claim ADR-0012 and SPEC §13.1 rest on: a bare room name authorizes the\n\
         /chat/v4 REST surface. The channel half (that the same resource also covers\n\
         `room::$chat`) is NOT observable from a REST-only client (ADR-0001) and\n\
         remains inference. This test asserts that row and only that row."
    );
    println!(
        "DENIED({CAPABILITY_DENIED}) means Ably refused the operation for want of\n\
         capability: in `send` the token lacked `publish` on a resource covering the\n\
         room, in `history` it lacked `history`, in `occupancy` it lacked\n\
         `channel-metadata`. A DENIED row therefore means that resource form does NOT\n\
         authorize the Chat REST API. On the `bare room name` row that would falsify\n\
         `Capability::for_room`."
    );
    println!(
        "READ THE COLUMNS BEFORE BLAMING THE RESOURCE FORM: a column DENIED on ALL\n\
         FOUR rows indicts the OPERATION mapping for that endpoint (publish for send,\n\
         history for history, channel-metadata for occupancy) — per-endpoint operation\n\
         requirements are strong inference in the memo, not verbatim documentation.\n\
         Only a column whose verdict DIFFERS BETWEEN rows is evidence about the\n\
         resource form."
    );
    println!(
        "The `::$chat channel suffix` row is EXPECTED to be DENIED({CAPABILITY_DENIED}):\n\
         it names only the realtime channel, and this crate is REST-only. Seeing it\n\
         succeed would mean the docs on `for_room` overstate the hazard. The other two\n\
         rows are diagnostics: they record which alternative forms also work."
    );
    println!(
        "ERR(...) with any code other than {CAPABILITY_DENIED} is probably NOT a\n\
         capability verdict — look at identity/clientId, token-issuance rate limiting\n\
         (40115 sits outside the renewal range), or app configuration first. Read the\n\
         code rather than dismissing it, though: an ERR at status 401/403 may still be\n\
         a permission refusal under a sibling error code.\n\
         MINT_FAILED means Ably rejected the token request itself, so nothing was\n\
         probed — a credential problem, never a verdict on the resource form. Read the\n\
         mint error printed on stderr above: 40400/40100 point at the key or app id\n\
         being wrong, while a capability complaint means the issuing key cannot\n\
         delegate what was requested. Use a key with `*` capability for this probe."
    );
    println!("--- END INTERPRETATION ---\n");

    let bare_row = &rows[0];
    // A rejected token request is a credential problem and must NOT be reported as
    // a failure of `for_room`, so it panics with its own message.
    if let Outcome::MintFailed {
        status,
        code,
        message,
    } = &bare_row.send
    {
        panic!(
            "could not mint a token for the bare room name `{room}` \
             (status={}, code={}): {message}. Nothing was probed, so this says \
             NOTHING about Capability::for_room — it is a credential problem. \
             Check that ABLY_API_KEY is a full `appId.keyId:keySecret` key for the \
             right app, and that its own capability grants at least \
             publish/history/channel-metadata on `*` (a token can only ever \
             receive the intersection of the requested and the key's capability).",
            opt(*status),
            opt(*code)
        );
    }
    assert!(
        bare_row.all_ok(),
        "LIVE VERIFICATION FAILED: a token scoped by the bare room name \
         (`{}`, as emitted by Capability::for_room) did not authorize all three \
         Chat REST operations — send={}, history={}, occupancy={}. \
         Capability::for_room is emitting the wrong resource form: ADR-0012 and \
         SPEC §13.1 must be corrected, and the helper changed (most likely to the \
         `[chat]{{room}}` product qualifier — check that row above), before 1.0.",
        bare_row.requested,
        bare_row.send.label(),
        bare_row.history.label(),
        bare_row.occupancy.label()
    );

    println!(
        "VERDICT: the bare room name authorizes the Chat REST surface \
         (send + history + occupancy) against a live app. The REST half of \
         ADR-0012 / SPEC §13.1 is confirmed. The `room::$chat` channel half is \
         untested here — a REST-only client cannot observe it — and still rests on \
         the [chat] product-qualifier model."
    );
}
