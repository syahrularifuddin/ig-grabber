"use client";

import { useEffect, useState } from "react";
import {
  apiGet,
  downloadUrl,
  fmtCount,
  parseUrl,
  proxyImg,
  sanitizeName,
  updateSession,
  SHORTCODE_RE,
  USERNAME_RE,
  type MediaItem,
  type PostResponse,
  type ProfileResponse,
  type SessionStatus,
  type StoryResponse,
} from "@/lib/api";

type Tab = "reels" | "photos" | "profile" | "story";

const TABS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: "reels", label: "Reels", icon: <IconClapperboard /> },
  { id: "photos", label: "Photos", icon: <IconImage /> },
  { id: "profile", label: "Profile Picture", icon: <IconUser /> },
  { id: "story", label: "Story", icon: <IconClock /> },
];

const PLACEHOLDERS: Record<Tab, string> = {
  reels: "Paste a reel URL (instagram.com/reel/…) or shortcode",
  photos: "Paste a photo URL (instagram.com/p/…) or shortcode",
  profile: "Enter a username or profile URL (e.g. nasa)",
  story: "Enter a username to fetch their active stories",
};

const POST_HINT: Record<"reels" | "photos", string> = {
  reels: "Paste a reel URL (instagram.com/reel/…) or a shortcode.",
  photos: "Paste a photo URL (instagram.com/p/…) or a shortcode.",
};

const KIND_LABEL: Record<string, string> = {
  image: "Photo",
  video: "Reel",
  carousel: "Carousel",
};

export default function Home() {
  const [tab, setTab] = useState<Tab>("reels");
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [post, setPost] = useState<PostResponse | null>(null);
  const [profile, setProfile] = useState<ProfileResponse | null>(null);
  const [story, setStory] = useState<StoryResponse | null>(null);
  const [session, setSession] = useState<SessionStatus | null>(null);
  const [sessionPanel, setSessionPanel] = useState(false);
  const [sidInput, setSidInput] = useState("");
  const [sidSaving, setSidSaving] = useState(false);
  const [sidMsg, setSidMsg] = useState<string | null>(null);

  useEffect(() => {
    apiGet<SessionStatus>("/api/session")
      .then(setSession)
      .catch(() => setSession(null));
  }, []);

  async function saveSession(e: React.FormEvent) {
    e.preventDefault();
    const sid = sidInput.trim();
    if (!sid) return;
    setSidSaving(true);
    setSidMsg(null);
    try {
      const status = await updateSession(sid);
      setSession(status);
      setSidInput("");
      setSidMsg("Session saved ✓");
    } catch (err) {
      setSidMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setSidSaving(false);
    }
  }

  async function go(rawOverride?: string) {
    const raw = (rawOverride ?? input).trim();
    if (!raw) return;

    const parsed = parseUrl(raw);
    // The URL wins over the active tab so pasting any link just works.
    const target: Tab = parsed?.type ?? tab;

    setError(null);
    setLoading(true);
    setTab(target);
    try {
      if (target === "reels" || target === "photos") {
        const shortcode =
          (parsed?.type === "reels" || parsed?.type === "photos")
            ? parsed.shortcode
            : bareShortcode(raw);
        if (!shortcode) throw new Error(POST_HINT[target]);
        setPost(await apiGet<PostResponse>(`/api/post/${encodeURIComponent(shortcode)}`));
      } else if (target === "profile") {
        const username =
          parsed?.type === "profile" ? parsed.username : bareUsername(raw);
        if (!username) throw new Error("Enter a profile username or URL.");
        setProfile(await apiGet<ProfileResponse>(`/api/profile/${encodeURIComponent(username)}`));
      } else {
        const username =
          parsed?.type === "story" ? parsed.username : bareUsername(raw);
        if (!username) throw new Error("Enter a username whose story you want to fetch.");
        setStory(await apiGet<StoryResponse>(`/api/stories/${encodeURIComponent(username)}`));
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  const isPostTab = tab === "reels" || tab === "photos";

  return (
    <main className="container">
      <div className="topbar">
        <div className="brand">
          <span className="brand-mark" />
          IG Grabber
        </div>
        {session && (
          <div className="session-row">
            <button
              type="button"
              className={`chip ${!session.configured ? "chip-off" : session.valid ? "chip-ok" : "chip-bad"}`}
              title="Instagram session cookie status — click to renew"
              onClick={() => setSessionPanel((v) => !v)}
            >
              <span className="chip-dot" />
              {!session.configured
                ? "no session — enable posts & stories"
                : session.valid
                  ? "session active"
                  : "session expired — renew"}
            </button>
          </div>
        )}
      </div>

      {session && (sessionPanel || !session.valid) && (
        <SessionPanel
          sidInput={sidInput}
          setSidInput={setSidInput}
          sidSaving={sidSaving}
          sidMsg={sidMsg}
          saveSession={saveSession}
        />
      )}

      <header className="hero">
        <h1>
          Grab any <em>Instagram</em> media.
        </h1>
        <p className="tagline">
          Reels, photos, covers, profile pictures &amp; stories — one paste, one click.
        </p>
      </header>

      <section className="tool" aria-label="Download tool">
        <nav className="tabs" role="tablist">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              className={`tab ${tab === t.id ? "active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.icon}
              {t.label}
            </button>
          ))}
        </nav>

        <form
          className="input-row"
          onSubmit={(e) => {
            e.preventDefault();
            void go();
          }}
        >
          <input
            className="input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={PLACEHOLDERS[tab]}
            spellCheck={false}
            autoFocus
          />
          <button className="btn primary" type="submit" disabled={loading}>
            {loading ? (
              <span className="spinner" aria-label="loading" />
            ) : (
              <>
                <IconDownload />
                Fetch
              </>
            )}
          </button>
        </form>

        {error && (
          <div className="error" role="alert">
            {error}
            {isPostTab && session && !session.configured && (
              <p className="error-hint">
                Instagram requires a logged-in session for posts &amp; reels — add{" "}
                <code>IG_SESSIONID</code> to <code>backend/.env</code> (steps in the Story tab).
              </p>
            )}
          </div>
        )}
      </section>

      {isPostTab && post && (
        <section className="result">
          {post.author_username && (
            <div className="author-row">
              {post.author_avatar && (
                <img
                  className="avatar-sm"
                  src={proxyImg(post.author_avatar)}
                  alt=""
                />
              )}
              <span className="author-name">@{post.author_username}</span>
              <span className={`kind-chip ${post.kind === "video" ? "reel" : post.kind === "image" ? "photo" : ""}`}>
                {KIND_LABEL[post.kind] ?? post.kind}
                {post.kind === "carousel" && ` · ${post.items.length} items`}
              </span>
            </div>
          )}
          <div className="media-grid">
            {post.items.map((item, i) => (
              <MediaCard key={i} item={item} baseName={sanitizeName(post.shortcode)} index={i} />
            ))}
          </div>
        </section>
      )}

      {tab === "profile" && profile && (
        <section className="result">
          <div className="profile-card">
            <div className="avatar-wrap">
              {(profile.profile_pic_url_hd ?? profile.profile_pic_url) && (
                <img
                  src={proxyImg(
                    profile.profile_pic_url_hd ?? profile.profile_pic_url!,
                  )}
                  alt=""
                />
              )}
            </div>
            <div className="profile-info">
              <h2>
                @{profile.username}
                {profile.is_verified && (
                  <span className="verified" title="Verified">
                    <IconBadgeCheck />
                  </span>
                )}
                {profile.is_private && <span className="badge private">PRIVATE</span>}
              </h2>
              {profile.full_name && <p className="full-name">{profile.full_name}</p>}
              {profile.biography && <p className="bio">{profile.biography}</p>}
              <div className="stats">
                <div className="stat">
                  <strong>{profile.posts != null ? fmtCount(profile.posts) : "—"}</strong>
                  <span>posts</span>
                </div>
                <div className="stat">
                  <strong>{profile.followers != null ? fmtCount(profile.followers) : "—"}</strong>
                  <span>followers</span>
                </div>
                <div className="stat">
                  <strong>{profile.following != null ? fmtCount(profile.following) : "—"}</strong>
                  <span>following</span>
                </div>
              </div>
              <div className="media-actions">
                {profile.profile_pic_url_hd ? (
                  <>
                    <a
                      className="btn primary"
                      href={downloadUrl(
                        profile.profile_pic_url_hd,
                        `${sanitizeName(profile.username)}_profile_pic_hd.jpg`,
                      )}
                    >
                      <IconDownload />
                      Download picture (HD)
                    </a>
                    {profile.profile_pic_url && (
                      <a
                        className="btn ghost"
                        href={downloadUrl(
                          profile.profile_pic_url,
                          `${sanitizeName(profile.username)}_profile_pic.jpg`,
                        )}
                      >
                        Standard quality
                      </a>
                    )}
                  </>
                ) : profile.profile_pic_url ? (
                  <a
                    className="btn primary"
                    href={downloadUrl(
                      profile.profile_pic_url,
                      `${sanitizeName(profile.username)}_profile_pic.jpg`,
                    )}
                  >
                    <IconDownload />
                    Download picture
                  </a>
                ) : null}
              </div>
            </div>
          </div>
        </section>
      )}

      {tab === "story" && story && (
        <section className="result">
          {!story.auth_configured ? (
            <div className="notice">
              <strong>Stories need a login session.</strong> Stories are only visible to
              logged-in accounts, so the backend needs a <code>sessionid</code> cookie from
              a <strong>throwaway account</strong> — automated traffic can get an account
              suspended, so never use your personal one:
              <ol>
                <li>Log in to instagram.com with a disposable account (no likes or follows)</li>
                <li>
                  Copy the <code>sessionid</code> cookie value (DevTools → Application → Cookies)
                </li>
                <li>
                  Click the session chip at the top and paste it — or put{" "}
                  <code>IG_SESSIONID</code> in <code>backend/.env</code>
                </li>
              </ol>
            </div>
          ) : story.items.length === 0 ? (
            <div className="empty">
              {story.note ?? `No active stories for @${story.username}.`}
            </div>
          ) : (
            <>
              <h3 className="result-title">{story.items.length} active story item(s)</h3>
              <div className="media-grid">
                {story.items.map((item, i) => (
                  <div className="media-card" key={item.id || i}>
                    <div className="media-preview">
                      <img src={item.thumbnail_url ?? item.url} alt="" loading="lazy" />
                      <span className={`badge ${item.kind}`}>{item.kind.toUpperCase()}</span>
                      {item.expires_at && (
                        <span className="expires">expires in {hoursLeft(item.expires_at)}</span>
                      )}
                    </div>
                    <div className="media-actions">
                      <a
                        className="btn primary"
                        href={downloadUrl(
                          item.url,
                          `${sanitizeName(story.username)}_story_${i + 1}${item.kind === "video" ? ".mp4" : ".jpg"}`,
                        )}
                      >
                        <IconDownload />
                        Download
                      </a>
                      <CopyLink url={item.url} />
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </section>
      )}

      <footer className="footer">
        Downloads publicly available media — for personal use. Respect creators&apos; copyright
        and Instagram&apos;s terms of service.
      </footer>
    </main>
  );
}

function bareShortcode(s: string): string | null {
  return SHORTCODE_RE.test(s) ? s : null;
}

function bareUsername(s: string): string | null {
  const u = s.replace(/^@/, "");
  return USERNAME_RE.test(u) ? u : null;
}

function hoursLeft(ts: number): string {
  const ms = ts * 1000 - Date.now();
  if (ms <= 0) return "soon";
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function SessionPanel({
  sidInput,
  setSidInput,
  sidSaving,
  sidMsg,
  saveSession,
}: {
  sidInput: string;
  setSidInput: (v: string) => void;
  sidSaving: boolean;
  sidMsg: string | null;
  saveSession: (e: React.FormEvent) => void;
}) {
  return (
    <form className="session-panel" onSubmit={saveSession}>
      <label>
        Paste the current <code>sessionid</code> cookie value — DevTools (F12) →
        Application → Cookies → instagram.com → <code>sessionid</code>. It replaces the
        old one in place; no restart needed. Use a <strong>throwaway account</strong>:
        automated traffic can get an account suspended.
      </label>
      <div className="input-row">
        <input
          className="input"
          value={sidInput}
          onChange={(e) => setSidInput(e.target.value)}
          placeholder="paste sessionid value here"
          spellCheck={false}
          autoComplete="off"
        />
        <button className="btn primary" type="submit" disabled={sidSaving || !sidInput.trim()}>
          {sidSaving ? <span className="spinner" aria-label="saving" /> : "Save session"}
        </button>
      </div>
      {sidMsg && <p className="sid-msg">{sidMsg}</p>}
    </form>
  );
}

function CopyLink({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      // clipboard unavailable (e.g. non-secure context) — do nothing
    }
  }

  return (
    <button type="button" className="btn ghost" onClick={() => void copy()}>
      {copied ? <IconCheck /> : <IconLink />}
      {copied ? "Copied" : "Copy link"}
    </button>
  );
}

/**
 * Reels and photos render different download options:
 *  - video → full MP4 + cover/thumbnail + copy direct link
 *  - image → full-size photo + copy direct link
 */
function MediaCard({ item, baseName, index }: { item: MediaItem; baseName: string; index: number }) {
  const isVideo = item.kind === "video";
  const mediaName = `${baseName}_${index + 1}${isVideo ? ".mp4" : ".jpg"}`;
  const thumbName = `${baseName}_${index + 1}_thumb.jpg`;
  const preview = isVideo ? item.thumbnail_url ?? null : item.url;

  return (
    <div className="media-card">
      <div className="media-preview">
        {preview ? (
          <img src={preview} alt="" loading="lazy" />
        ) : (
          <div className="preview-empty">no preview</div>
        )}
        <span className={`badge ${isVideo ? "video" : "image"}`}>
          {isVideo ? "REEL" : "PHOTO"}
        </span>
        {item.width && item.height && (
          <span className="dims">
            {item.width}×{item.height}
          </span>
        )}
      </div>
      <div className="media-actions">
        {isVideo ? (
          <>
            <a className="btn primary" href={downloadUrl(item.url, mediaName)}>
              <IconDownload />
              Video (MP4)
            </a>
            {item.thumbnail_url && (
              <a className="btn ghost" href={downloadUrl(item.thumbnail_url, thumbName)}>
                <IconImage />
                Cover
              </a>
            )}
          </>
        ) : (
          <a className="btn primary" href={downloadUrl(item.url, mediaName)}>
            <IconDownload />
            Photo (full size)
          </a>
        )}
        <CopyLink url={item.url} />
      </div>
    </div>
  );
}

/* ---------- Icons (Lucide, inline SVG) ---------- */

function Svg({ children }: { children: React.ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

function IconDownload() {
  return (
    <Svg>
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="7 10 12 15 17 10" />
      <line x1="12" x2="12" y1="15" y2="3" />
    </Svg>
  );
}

function IconLink() {
  return (
    <Svg>
      <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
      <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
    </Svg>
  );
}

function IconCheck() {
  return (
    <Svg>
      <polyline points="20 6 9 17 4 12" />
    </Svg>
  );
}

function IconImage() {
  return (
    <Svg>
      <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
      <circle cx="9" cy="9" r="2" />
      <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
    </Svg>
  );
}

function IconClapperboard() {
  return (
    <Svg>
      <path d="M20.2 6 3 11l-.9-2.4c-.3-1.1.3-2.2 1.3-2.5l13.5-4c1.1-.3 2.2.3 2.5 1.3Z" />
      <path d="m6.2 5.3 3.1 3.9" />
      <path d="m12.4 3.4 3.1 4" />
      <path d="M3 11h18v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
    </Svg>
  );
}

function IconUser() {
  return (
    <Svg>
      <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </Svg>
  );
}

function IconClock() {
  return (
    <Svg>
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </Svg>
  );
}

function IconBadgeCheck() {
  return (
    <Svg>
      <path d="M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1-4.78-4.77 4 4 0 0 1 0-6.76Z" />
      <path d="m9 12 2 2 4-4" />
    </Svg>
  );
}
