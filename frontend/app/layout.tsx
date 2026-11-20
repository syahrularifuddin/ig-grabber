import type { Metadata, Viewport } from "next";
import { Inter, Playfair_Display, JetBrains_Mono } from "next/font/google";
import "./globals.css";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-sans",
});

const playfair = Playfair_Display({
  subsets: ["latin"],
  style: ["italic"],
  weight: ["700"],
  variable: "--font-display",
});

const mono = JetBrains_Mono({
  subsets: ["latin"],
  weight: ["400"],
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
  themeColor: "#e11d48",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className={`${inter.variable} ${playfair.variable} ${mono.variable}`}>{children}</body>
    </html>
  );
}
