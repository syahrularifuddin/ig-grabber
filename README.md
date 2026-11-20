# IG Grabber

A full-stack web app to download publicly available Instagram media:

- **Reels & videos**
- **Photos** (including every item of a carousel)
- **Reel thumbnails**
- **Profile pictures** (HD when available)
- **Stories** (requires a logged-in `sessionid` cookie — see below)

```
ig/
├── backend/    Rust (axum + wreq) API server
└── frontend/   Next.js (TypeScript) web UI
```

## How it works

Instagram fingerprints HTTP clients (TLS/HTTP2), so plain scrapers get challenge pages. The
backend uses [`wreq`](https://crates.io/crates/wreq) — an HTTP client that reproduces a real
browser's TLS/HTTP2 fingerprint — and tries multiple extraction strategies per request
(private JSON API → GraphQL → embed page → og: meta tags). Media is proxied through the
backend (`/download`) so the browser downloads files without CORS problems.

## Quickstart

**Backend** (Rust 1.98+ via [rustup](https://rustup.rs); macOS builds BoringSSL, needs cmake):

```bash
cd backend
cp .env.example .env   # optional: set PORT / IG_SESSIONID once
cargo run              # listens on http://localhost:8080
```

**Frontend** (Node 18.18+):

```bash
cd frontend
npm install
npm run dev          # http://localhost:3000 (proxies /api and /download to :8080)
```

Then open http://localhost:3000, paste a reel/photo/profile/story URL or username, and hit
**Fetch**. Pasting any supported URL auto-routes to the right tab. The header shows your
session status (`/api/session`).

## Configuration

| Variable       | Where     | Purpose                                                        |
| -------------- | --------- | -------------------------------------------------------------- |
| `PORT`         | backend   | Port the API listens on (default `8080`)                       |
| `IG_SESSIONID` | backend   | Instagram `sessionid` cookie — **required for posts & stories**|
| `BACKEND_URL`  | frontend  | Where the Next.js proxy forwards API calls (default `:8080`)   |

The backend reads a local `backend/.env` file (see `.env.example`) — set variables once and
`cargo run` just works. Real environment variables take precedence.

### Posts, reels & stories need a session

Instagram blocks anonymous extraction of posts and stories (rate-limit walls and
`login_required`); only profile data and profile pictures work without login. To enable
everything else with **your own login** (personal use):

1. Log in to instagram.com in your browser — the app never sees your password.
2. DevTools (F12) → Application → Cookies → `https://www.instagram.com` → copy the
   `sessionid` value.
3. Put it in `backend/.env`:
   ```
   IG_SESSIONID=your_value_here
   ```
4. Restart the backend (`cargo run`). The header chip in the UI should turn green —
   "session active".

The cookie is a session token, not credentials: it stays in your local `.env`, is sent only
to Instagram, and can be revoked anytime by logging out or changing your password.
A `sessionid` can also be supplied per-request via the `x-session-id` header, and private
accounts require the configured session to follow them.

### Session expiry & renewal

The cookie eventually expires (typically weeks to months; immediately on password change or
"log out of all sessions"), and Instagram sometimes silently *rolls* the value. The app
handles both:

- **Auto-renewal**: when Instagram rolls the cookie on any authenticated response, the
  backend adopts the new value and rewrites `.env` — nothing to do.
- **In-app renewal**: if the chip turns red ("session expired"), click it, paste a fresh
  `sessionid` from your logged-in browser, and it's validated, swapped in live, and
  persisted — no restart, no terminal.

## API

| Endpoint                          | Description                                    |
| --------------------------------- | ---------------------------------------------- |
| `GET /api/profile/{username}`     | Profile info + profile picture URLs            |
| `GET /api/post/{shortcode}`       | Post/reel media items (photo, video, carousel) |
| `GET /api/stories/{username}`     | Active stories (`auth_configured` flag)        |
| `GET /api/session`                | Whether the session cookie is configured+valid |
| `POST /api/session`               | Renew the cookie live (`{"sessionid": "…"}`)   |
| `GET /download?url=…&filename=…`  | Streams a CDN URL as a file attachment         |
| `GET /health`                     | Liveness check                                 |

All `/download` requests are restricted to Instagram CDN hosts (`*.cdninstagram.com`,
`*.fbcdn.net`) as an SSRF guard.

## Notes & limitations

- **Personal use only.** Downloading content you don't own may violate Instagram's Terms of
  Service and creators' copyright — get permission before reusing anything.
- Extraction depends on Instagram's unauthenticated endpoints; they change without notice and
  some networks/IPs get rate-limited. The UI surfaces those errors verbatim.
- Stories always require the `sessionid` cookie, by Instagram's design.
