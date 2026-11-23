# IG Grabber

[![CI & Deploy](https://github.com/syahrularifuddin/ig-grabber/actions/workflows/deploy.yml/badge.svg)](https://github.com/syahrularifuddin/ig-grabber/actions/workflows/deploy.yml)
[![Rust](https://img.shields.io/badge/Rust-1.98%2B-DEA584?logo=rust&logoColor=black)](https://www.rust-lang.org)
[![Node.js](https://img.shields.io/badge/Node.js-18.18%2B-339933?logo=nodedotjs&logoColor=white)](https://nodejs.org)
[![Next.js](https://img.shields.io/badge/Next.js-TypeScript-000000?logo=nextdotjs&logoColor=white)](https://nextjs.org)
[![Docker](https://img.shields.io/badge/Docker-GHCR-2496ED?logo=docker&logoColor=white)](https://ghcr.io/syahrularifuddin/ig-grabber)

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

### Posts, reels & stories need a session (use a burner account)

Instagram blocks anonymous extraction of posts and stories (rate-limit walls and
`login_required`); only profile data and profile pictures work without login. To enable
everything else, use a **throwaway account — never your personal one**. Automated traffic
can get an account suspended, and a burner contains that blast radius.

1. Create a disposable Instagram account and log in with it in your browser. Skip the
   profile setup, and never use it for likes, follows or comments.
2. DevTools (F12) → Application → Cookies → `https://www.instagram.com` → copy the
   `sessionid` value.
3. Click the session chip in the UI and paste it (validated + saved live) — or put it in
   `backend/.env`:
   ```
   IG_SESSIONID=your_burner_value_here
   ```
4. The chip should turn green — "session active". If it goes red, the account/cookie is
   dead; make another burner, don't retry the old one in a loop.

The backend spaces out authenticated calls (~1.2s apart) and never follows, likes or
posts — but treat the burner as disposable either way. The cookie is a session token, not
credentials: it stays in your local `.env` and is sent only to Instagram.
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

## Deployment

Push to `main` (PR merge or direct push) deploys. GitHub Actions runs the CI checks
first, builds the backend and frontend images, pushes them to GHCR (tagged `latest` +
git SHA), then SSHes to the VPS and runs `docker compose pull && up -d`. Failing checks
block the deploy; back-to-back merges queue instead of racing.

The app has **no public ports** of its own. It joins the shared `edge` Docker network of
the VPS that already serves [halosyahrul.com](https://halosyahrul.com), whose Caddy
terminates TLS and routes **https://ig.halosyahrul.com**:

- `/api/*` and `/download` → `ig-api:8080`
- everything else → `ig-web:3000`

One-time VPS setup (the halosyahrul.com stack must already be running there):

```bash
sudo mkdir -p /opt/ig && sudo chown "$USER" /opt/ig
cp deploy/.env.example /opt/ig/.env
sudo chown 65534:65534 /opt/ig/.env   # backend (uid 65534) rewrites it on session renewal
# deploy-only SSH key: put its public half in ~/.ssh/authorized_keys
```

Add repo **secrets** (`Settings → Secrets → Actions`): `VPS_HOST`, `VPS_USER`,
`VPS_SSH_KEY`. Optional: `VPS_PORT` (default 22), `VPS_TARGET_PATH` (default `/opt/ig`).

Add the DNS record `ig.halosyahrul.com A <VPS_IP>` (Caddy gets the Let's Encrypt cert
automatically), and the `ig.{$DOMAIN}` block to halosyahrul.com's `deploy/Caddyfile`
(that repo owns the edge) — that block is what points the subdomain at
`ig-api`/`ig-web`. Then `git push` ships.

Rollback: `docker images | grep ig-grabber` to find the previous SHA tag, then
`docker tag <old> ghcr.io/syahrularifuddin/ig-grabber/backend:latest && docker compose up -d ig-api`.

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
