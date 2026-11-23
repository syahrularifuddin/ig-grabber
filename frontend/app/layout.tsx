import type { Metadata, Viewport } from "next";
import { JetBrains_Mono, Plus_Jakarta_Sans, Space_Grotesk } from "next/font/google";
import "./globals.css";

// Same three families as halosyahrul.com: Plus Jakarta Sans (body),
// Space Grotesk (display), JetBrains Mono (labels/meta).
const sans = Plus_Jakarta_Sans({
  subsets: ["latin"],
  weight: ["400", "500", "600", "700"],
  variable: "--font-sans",
});

const display = Space_Grotesk({
  subsets: ["latin"],
  weight: ["500", "600", "700"],
  variable: "--font-display",
});

const mono = JetBrains_Mono({
  subsets: ["latin"],
  weight: ["400", "500"],
  variable: "--font-mono",
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
