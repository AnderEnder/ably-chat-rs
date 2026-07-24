# ADR-0012: Token issuance & permission helpers (capabilities, Ably JWT, refresh provider)

- Status: Proposed
- Date: 2026-07-24
- Deciders: Andrii Radyk
- Relates to: [ADR-0005](0005-authentication.md) (extends its reserved `TokenProvider` point), [ADR-0001](0001-rest-only-scope.md) (REST-only boundary)
- Research: [`../research/2026-07-24-ably-chat-auth-permissions.md`](../research/2026-07-24-ably-chat-auth-permissions.md)
- Amended 2026-07-24: decision item 5 revised — the platform token endpoints are now
  modeled in a separate spec ([`../../openapi/ably-auth-rest.yaml`](../../openapi/ably-auth-rest.yaml))
  and generated as the `ably-auth-openapi` crate, partially reversing the original "decline".

## Context

[ADR-0005](0005-authentication.md) accepted **static credentials only** — an `Auth`
enum of `ApiKey` (Basic) or `Token` (Bearer) — and reserved a `TokenProvider` point
for later. This ADR decides how much of Ably's authentication/permission surface the
crate should own. Research established the relevant facts:

- **Permissions in Ably are *capabilities***: a JSON map from channel-name patterns to
  a list of **operations** (17 in total; the chat-relevant ones are `publish`,
  `history`, `message-update-own`/`-any`, `message-delete-own`/`-any`,
  `annotation-publish`, `channel-metadata`, plus realtime-only `subscribe`/`presence`).
  A token's effective capability is the **intersection** of the requested capability
  and the issuing key's own. There is **no** role system and no `moderation` operation —
  "moderator" is a token holding the `-any` variants.
- **Chat rooms are channel-backed**: room `R` → channel `R::$chat`. A single room-scoped
  capability resource authorises both the `/chat/v4` REST API and the channel. Scope by
  **room name** — bare `"R"` or the explicit product qualifier `"[chat]R"` — **not**
  `"R::$chat"` (which authorises only the realtime channel and would `40160` on REST).
- **Token issuance is a *platform* concern, not a chat one.** `POST
  /keys/{keyName}/requestToken` and `POST /keys/{keyName}/revokeTokens` live on the
  platform REST host (`rest.ably.io`), outside `/chat/v4`. Ably's own Chat SDK
  "doesn't handle authentication directly — it uses the authenticated connection from
  the underlying Pub/Sub client."
- **Two token styles**: an **Ably JWT** (HS256, `kid` = key name, `x-ably-capability` /
  `x-ably-clientId` claims) can be minted locally with **no network call and no
  capability canonicalization**; a native **Ably Token** requires signing a TokenRequest
  (fiddly canonical HMAC string) and exchanging it at `requestToken`.
- **Prior art**: the official `ably` crate (REST-only, v0.2.0, actively maintained)
  already implements TokenRequest signing, `request_token`, and an `AuthCallback` — but
  models capability as a raw `String` and cannot mint JWTs. No Rust crate offers typed
  capabilities or JWT minting; both are greenfield. Depending on `ably` is not viable
  today (it pins `reqwest 0.11`/`base64 0.13` vs this crate's `0.13`/`0.22`).

## Decision

Adopt a tiered, additive, feature-gated design. The crate stays a Chat-REST client and
owns only the *safe, high-value, greenfield* pieces; platform token-issuance machinery is
delegated.

1. **Keep** `Auth::ApiKey` / `Auth::Token` unchanged (ADR-0005). This already covers the
   dominant server-side case, where the key's dashboard-configured capability is enforced
   server-side and no minting is needed.

2. **Typed capability model** — Cargo feature `capabilities` (**on by default**;
   disable via `default-features = false`). An
   `Operation` enum (the 17 operations) and a `Capability` value type
   (`resource pattern → set of operations`) with a `for_room(room, ops)` helper that
   scopes to the room name (bare, or `[chat]`-qualified) — never `::$chat`. Two
   serializers: the **canonical string** (sorted resource keys, sorted operations, no
   whitespace) required by TokenRequest/JWT, and a **native JSON object** for the Ably
   **Control API** key endpoints. Mints nothing → no secret handling.

3. **Ably JWT minter** — Cargo feature `jwt` (**on by default**; disable via
   `default-features = false`), server-side only. A
   `mint_ably_jwt(api_secret, params)` producing an HS256 JWT (`kid` = key name; claims
   `x-ably-capability`, optional `x-ably-clientId`, `iat`, `exp`, optional
   `x-ably-revocation-key`). No network call; **no canonicalization needed** (the
   signature covers the whole payload). Guarded by an `ApiSecret` newtype with a redacting
   `Debug` and prominent "server-side only" documentation.

4. **`Auth::Provider(Arc<dyn TokenProvider>)`** — the reserved refresh seam. The dispatch
   layer resolves the `Authorization` header per request from a cached, provider-supplied
   Bearer credential, and on an HTTP `401` whose Ably error code is in `[40140, 40150)`
   makes **exactly one** re-auth attempt and retries once (spec RSA4b); it never loops.
   Refresh is single-flighted — including the 401-triggered *forced* refresh, which
   dedupes via a staleness token (a caller passes the header it had rejected; if the cache
   has already moved on, the fresh value is reused instead of minting again). This is a
   deliberate choice to go *beyond* the Ably spec — see the consequence note below.
   Adding this variant makes `Auth` `#[non_exhaustive]`.

5. **Model the platform token endpoints in a separate spec + generated crate** *(revised
   2026-07-24; originally "decline")*. The token-issuance/revocation/time endpoints
   (`POST /keys/{keyName}/requestToken`, `POST /keys/{keyName}/revokeTokens`, `GET /time`)
   are described in a dedicated OpenAPI document,
   [`../../openapi/ably-auth-rest.yaml`](../../openapi/ably-auth-rest.yaml), and generated
   into a bindings crate **`ably-auth-openapi`** (`ably_auth_openapi`) that mirrors
   `ably-chat-openapi` (same generator + pinned version, drift-gated `src/`, hand-managed
   `Cargo.toml`/`README`). This provides typed request/response bindings for the exchange
   **without** a dependency on the `ably` crate.

   Still deliberately **out of scope for now**: (a) the fragile **TokenRequest HMAC
   signing** — a caller either sends an unsigned `TokenParams` with Basic auth (the
   generated client supports this) or supplies a pre-signed `TokenRequest`; and (b) an
   ergonomic wrapper over these bindings in `ably-chat-rs` (a `TokenProvider` that performs
   the exchange). The crate still does not depend on the `ably` crate (version skew).

## Consequences

- A Rust *auth server* can construct correctly-typed, correctly-scoped capabilities and
  mint scoped Ably JWTs for its end users — the officially recommended pattern — without
  hand-rolling JSON or JWT claims, and with the API secret confined behind a feature flag
  and a redacting newtype.
- Expiring-token callers gain automatic refresh via `Auth::Provider`; the dispatch hot
  path changes from a precomputed static header to a provider-resolved one plus a
  `401`→re-auth→single-retry path (which does not exist today).
- `Auth` becomes `#[non_exhaustive]`; callers can no longer match it exhaustively. This is
  a one-time source-compatibility cost, sequenced before it would otherwise break callers.
- The capability model is reusable for the **Control API** (key create/update), giving the
  type a second consumer.
- **Both helpers are on by default** (differing from the research memo's off-by-default
  recommendation — a deliberate maintainer choice for discoverability). Consequence: `jwt`
  default-on pulls `jsonwebtoken` into every build and widens the secret-handling surface;
  the mitigations are the `ApiSecret` redacting newtype and prominent "server-side only"
  documentation, not the feature gate. A minimal-footprint consumer opts out with
  `default-features = false`. `capabilities` default-on has no secret and negligible weight.
- **MSRV raised to 1.88** (from 1.85): `jsonwebtoken 9` → `simple_asn1` → `time 0.3.54`
  requires rustc 1.88, so keeping `jwt` default-on forces the bump. Applied to the workspace
  `rust-version`, the CI `msrv` job, SPEC §12, and the crate READMEs; supersedes ADR-0011's
  MSRV clause. (Alternative considered: hand-roll HS256 with `hmac`/`sha2` to keep 1.85 —
  rejected in favour of the well-tested `jsonwebtoken`.)
- Native Ably Token issuance is now reachable in-repo via the generated `ably-auth-openapi`
  bindings (the `requestToken`/`revokeTokens` exchange), not only via the `ably` crate.
  TokenRequest HMAC *signing* remains the caller's responsibility (or use unsigned
  `TokenParams` + Basic auth); an ergonomic wrapper is future work.
- **Single-flight refresh is ours, not Ably's — and is load-bearing.** The Ably features
  spec does **not** require deduplicating concurrent renewals: RSA4b's "a single attempt"
  is per-request, and a sweep of `ably/specification` finds no concurrency clause in
  RSA4/RSA8/RSA9/RSA10/RSA16. No official SDK fully dedupes the forced (401-driven) case
  either — `ably-js` piggybacks only *non-forced* callers (forced ones fan out, "last one
  wins"); `ably-go` serializes on a mutex but still re-requests per caller; `ably-java` and
  `ably-python` have no protection. We nevertheless require it (SPEC §13.3) because the
  naive forced path is worse than redundant: holding the cache mutex across the provider
  call turns N concurrent 401s into N *sequential* mints, so tail latency grows with
  concurrency — a latency cliff at every token expiry for the server-side `Arc<Client>`
  case this ADR targets. Ably also meters token issuance separately (**50 req/s** and 60k/h
  on Free; 250/s Standard; 500/s Pro), and exceeding it returns **`40115`
  `account_request_limit_exceeded`**, which sits *outside* the `[40140, 40150)` renewal
  range — so a large enough burst degrades into failures the retry-once path cannot
  recover. Deduping removes both failure modes at the cost of ~10 lines.
- **Open verification (blocks *stabilization* of the `capabilities` `for_room` helper;
  it ships pre-1.0 with the caveat documented on the helper itself):** the exact
  resource string that authorises both the `::$chat` channel and the `/chat/v4` REST calls
  is confirmed only to moderate-high confidence (the `[chat]` qualifier is documented in a
  0.7-era note). A live test matrix (`[chat]{room}` · bare `{room}` · `{room}::$chat` ·
  `{room}:*`, checking send/history/occupancy for `40160`) MUST be run before 1.0.

## Alternatives considered

- **Build the full auth stack here** (TokenRequest signing + `requestToken` + revocation) —
  rejected: platform-REST scope creep past ADR-0001, silent-failure risk, and redundant
  with the `ably` crate.
- **Do nothing beyond ADR-0005** (accept only externally-minted tokens) — rejected: leaves
  every Rust chat auth server to hand-roll capability JSON and JWT claims, the one thing no
  Rust crate provides.
- **Depend on the official `ably` crate for auth** — rejected for now: dependency-graph
  collision (`reqwest 0.11`/`base64 0.13` vs `0.13`/`0.22`) plus an unwanted crypto/msgpack
  footprint. Revisit if `ably` upgrades or exposes a slim, default-features-off auth slice.
- **A dedicated shared auth crate** (capabilities + JWT extracted upstream) — noted as the
  principled long-term home; deferred because no such crate exists and callers need these
  now. Tiers 1–2 are to be designed so they can later move without breaking this crate's
  public surface.
- **Model the platform auth endpoints as a separate generated crate** *(chosen for item 5)*
  — a middle path between hand-writing the fragile full auth stack and doing nothing: typed
  bindings via the same codegen pipeline as the Chat crate, no `ably` dependency, with
  TokenRequest HMAC signing still left to the caller.
