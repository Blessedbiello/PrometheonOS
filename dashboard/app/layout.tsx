import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "PrometheonOS — Execution Intelligence",
  description: "Autonomous Solana execution intelligence — live operational telemetry.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="ops-bg min-h-screen">{children}</body>
    </html>
  );
}
