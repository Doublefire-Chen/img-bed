import { readFileSync } from "node:fs";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as {
  version: string;
};

// The server serves the built files under /admin/ (a Reserved Prefix).
export default defineConfig({
  base: "/admin/",
  plugins: [react()],
  define: {
    // Compared with the server's version from /api/v1/info; keep it equal
    // to the version in server/Cargo.toml.
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  server: {
    // `pnpm dev` talks to a locally running `img-bed serve`.
    proxy: { "/api": "http://127.0.0.1:8080" },
  },
});
