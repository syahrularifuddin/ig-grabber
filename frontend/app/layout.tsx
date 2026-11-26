import type { Metadata, Viewport } from "next";
import localFont from "next/font/local";
import "./globals.css";

// Same three families as halosyahrul.com: Plus Jakarta Sans (body),
// Space Grotesk (display), JetBrains Mono (labels/meta). Self-hosted: Google
// Fonts intermittently answers `css2` with extensionless /l/font?kit= URLs,
// which crashes next/font/google at build time (vercel/next.js#99114).
const sans = localFont({
  src: "./fonts/plus-jakarta-sans-latin.woff2",
  weight: "400 700",
  variable: "--font-sans",
  display: "swap",
});

const display = localFont({
  src: "./fonts/space-grotesk-latin.woff2",
  weight: "500 700",
  variable: "--font-display",
  display: "swap",
});

const mono = localFont({
  src: "./fonts/jetbrains-mono-latin.woff2",
  weight: "400 500",
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: "IG Grabber — Instagram Downloader",
  description: "Download Instagram reels, photos, thumbnails, profile pictures and stories.",
  manifest: "/manifest.webmanifest",
  // Full icon set, declared explicitly (file conventions would override this
  // block — and Next dedupes icon.svg vs icon.png, dropping the SVG link):
  // crisp SVG for modern browsers, PNG fallbacks for Safari, apple-touch for
  // iOS home screen. Maskable/512 variants ship via the web manifest.
  icons: {
    icon: [
      { url: "/favicon.svg", type: "image/svg+xml" },
      { url: "/icons/icon-192.png", sizes: "192x192", type: "image/png" },
      { url: "/icons/icon-512.png", sizes: "512x512", type: "image/png" },
    ],
    apple: "/icons/apple-touch-icon.png",
  },
};

export const viewport: Viewport = {
  themeColor: "#faf6ef",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className={`${sans.variable} ${display.variable} ${mono.variable}`}>
        {children}
      </body>
    </html>
  );
}
