import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { Providers } from "@/components/Providers";
import { Shell } from "@/components/Shell";
import "../styles/site.css";
import "@keyless-trust/ui/tokens.css";
import "../styles/app.css";

export const metadata: Metadata = {
  title: "Keyless Trust · by Ethics",
  description:
    "Community token raises whose Solana vaults are owned by a NEAR contract. Testnet and Solana devnet only.",
};

export const viewport: Viewport = {
  themeColor: "#0B0D0F",
  colorScheme: "dark",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className="dark">
      <head>
        <link rel="icon" href="/brand/ethics-monogram.svg" type="image/svg+xml" />
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="" />
        <link
          href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:wght@500;600&family=DM+Sans:wght@400;500;600&display=swap"
          rel="stylesheet"
        />
      </head>
      <body
        className="antialiased"
        style={{
          ["--font-cormorant" as string]: '"Cormorant Garamond", Georgia, serif',
          ["--font-dm-sans" as string]: '"DM Sans", system-ui, sans-serif',
        }}
      >
        <Providers>
          <Shell>{children}</Shell>
        </Providers>
      </body>
    </html>
  );
}
