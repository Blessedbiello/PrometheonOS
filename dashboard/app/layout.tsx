import type { Metadata } from "next";
import { JetBrains_Mono, Space_Grotesk } from "next/font/google";
import "./globals.css";

const mono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono", display: "swap" });
const grotesk = Space_Grotesk({
  subsets: ["latin"],
  weight: ["500", "600", "700"],
  variable: "--font-grotesk",
  display: "swap",
});

export const metadata: Metadata = {
  title: "PrometheonOS — Execution Control Plane",
  description:
    "Autonomous Solana execution intelligence — watch an AI operate a Jito transaction stack and self-heal failures, live.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${mono.variable} ${grotesk.variable}`}>
      <body className="instrument min-h-screen">{children}</body>
    </html>
  );
}
