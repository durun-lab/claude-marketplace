// PWA を dist/ に出す。`GET /m/` はこの dist/ をそのまま返す（docs/design/12-mobile.md「HTTP の形」）。
// 使い方: node build.mjs（ui/mobile/ で）。出力は index.html、demo.html、app.js、sw.js、manifest、アイコン。
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { URL, fileURLToPath } from "node:url";
import { build } from "esbuild";
import { makeIcon } from "./icons.mjs";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const DIST = here("./dist/");

const ICONS = [
  ["icon-192.png", 192, 0.5],
  ["icon-512.png", 512, 0.5],
  ["icon-maskable-512.png", 512, 0.36],
  ["apple-touch-icon.png", 180, 0.5],
];

const page = (name) => `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Movie Maker</title>
<meta name="theme-color" content="#101012">
<meta name="mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-title" content="Movie Maker">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<link rel="manifest" href="./manifest.webmanifest">
<link rel="apple-touch-icon" href="./apple-touch-icon.png">
<link rel="stylesheet" href="./${name}.css">
</head>
<body>
<div id="root"></div>
<script src="./${name}.js"></script>
</body>
</html>
`;

const common = { bundle: true, format: "iife", target: ["safari15", "chrome100"], jsx: "automatic", jsxImportSource: "preact", minify: true, logLevel: "warning" };

const hashOf = async (files) => {
  const h = createHash("sha256");
  for (const f of files) h.update(await readFile(`${DIST}${f}`));
  return h.digest("hex").slice(0, 12);
};

await rm(DIST, { recursive: true, force: true });
await mkdir(DIST, { recursive: true });
await build({ ...common, entryPoints: { app: here("./src/main.tsx"), demo: here("./demo/main.tsx") }, outdir: DIST });
await cp(here("./public/manifest.webmanifest"), `${DIST}manifest.webmanifest`);
for (const [file, size, scale] of ICONS) await writeFile(`${DIST}${file}`, makeIcon(size, scale));
await writeFile(`${DIST}index.html`, page("app"));
await writeFile(`${DIST}demo.html`, page("demo"));

const assets = ["./", "./index.html", "./app.js", "./app.css", "./manifest.webmanifest", ...ICONS.map(([f]) => `./${f}`)];
const version = await hashOf(["app.js", "app.css", "index.html", "manifest.webmanifest"]);
await build({
  ...common,
  entryPoints: { sw: here("./src/sw/sw.ts") },
  outdir: DIST,
  define: { __CACHE_NAME__: JSON.stringify(`mm-mobile-${version}`), __ASSETS__: JSON.stringify(assets) },
});
