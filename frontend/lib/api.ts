export type MediaKind = "image" | "video";
export type PostKind = "image" | "video" | "carousel";

export interface MediaItem {
  kind: MediaKind;
  url: string;
  thumbnail_url?: string;
  width?: number;
  height?: number;
}

export interface PostResponse {
  shortcode: string;
  kind: PostKind;
  caption?: string;
  author_username?: string;
  author_avatar?: string;
  items: MediaItem[];
}

export interface ProfileResponse {
  user_id: string;
  username: string;
  full_name?: string;
  biography?: string;
  profile_pic_url?: string;
  profile_pic_url_hd?: string;
  is_private: boolean;
  is_verified: boolean;
  followers?: number;
  following?: number;
  posts?: number;
}

export interface StoryItem {
  id: string;
  kind: MediaKind;
  url: string;
  thumbnail_url?: string;
  width?: number;
  height?: number;
  taken_at?: number;
  expires_at?: number;
}

export interface StoryResponse {
  username: string;
  /** Whether the backend has a sessionid cookie configured (required for stories). */
  auth_configured: boolean;
  /** Optional explanation (e.g. why the item list is empty). */
  note?: string;
  items: StoryItem[];
}

export interface SessionStatus {
  /** Whether IG_SESSIONID is configured in the backend. */
  configured: boolean;
  /** Whether Instagram currently accepts the cookie. */
  valid: boolean;
}

/** Validate + hot-swap the backend session cookie (no restart needed). */
export async function updateSession(sessionid: string): Promise<SessionStatus> {
  const res = await fetch("/api/session", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ sessionid }),
  });
  const data = await res.json().catch(() => null);
  if (!res.ok) {
    const msg = (data as { error?: string } | null)?.error;
    throw new Error(msg ?? `Failed (HTTP ${res.status})`);
  }
  return data as SessionStatus;
}

export type ParsedInput =
  | { type: "reels"; shortcode: string }
  | { type: "photos"; shortcode: string }
  | { type: "profile"; username: string }
  | { type: "story"; username: string };

export const USERNAME_RE = /^[A-Za-z0-9._]{1,30}$/;
export const SHORTCODE_RE = /^[A-Za-z0-9_-]{5,32}$/;

/** Parse a pasted Instagram URL (reel/photo/profile/story) into a structured target. */
export function parseUrl(raw: string): ParsedInput | null {
  const input = raw.trim();
  if (!input) return null;
  try {
    const url = new URL(input.startsWith("http") ? input : `https://${input}`);
    if (!/(^|\.)instagram\.com$/i.test(url.hostname)) return null;
    const parts = url.pathname.split("/").filter(Boolean);
    if (["p", "tv"].includes(parts[0]) && parts[1] && SHORTCODE_RE.test(parts[1])) {
      return { type: "photos", shortcode: parts[1] };
    }
    if (["reel", "reels"].includes(parts[0]) && parts[1] && SHORTCODE_RE.test(parts[1])) {
      return { type: "reels", shortcode: parts[1] };
    }
    if (parts[0] === "stories" && parts[1] && USERNAME_RE.test(parts[1])) {
      return { type: "story", username: parts[1] };
    }
    const reserved = ["explore", "accounts", "direct", "your_activity", "stories", "p", "reel", "reels", "tv"];
    if (parts[0] && !reserved.includes(parts[0]) && USERNAME_RE.test(parts[0])) {
      return { type: "profile", username: parts[0] };
    }
  } catch {
    // not a URL
  }
  return null;
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(path, { headers: { accept: "application/json" } });
  let data: unknown = null;
  try {
    data = await res.json();
  } catch {
    // non-JSON error body
  }
  if (!res.ok) {
    const msg = (data as { error?: string } | null)?.error;
    throw new Error(msg ?? `Request failed (HTTP ${res.status})`);
  }
  return data as T;
}

/** Same-origin download proxy URL (streams through the backend, no CORS issues). */
export function downloadUrl(url: string, filename: string): string {
  return `/download?url=${encodeURIComponent(url)}&filename=${encodeURIComponent(filename)}`;
}

/**
 * Same proxy as a plain <img> src: Instagram's CDN serves browsers a
 * non-image response for some URL classes (e.g. profile pics) but serves
 * them fine to the backend's client, so avatars must load via the proxy.
 */
export function proxyImg(url: string, name = "avatar.jpg"): string {
  return downloadUrl(url, name);
}

export function fmtCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\.0$/, "")}K`;
  return String(n);
}

export function sanitizeName(s: string): string {
  return (
    s
      .replace(/[^A-Za-z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 60) || "download"
  );
}
