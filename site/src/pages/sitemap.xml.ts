import type { APIRoute } from "astro";

export const prerender = true;

const site = "https://reviewgate.lvtd.dev";
const pageFiles = import.meta.glob(["./**/*.astro", "./**/*.md"]);

const toRoute = (file: string) => {
  const page = file.replace(/^\.\//, "").replace(/\.(astro|md)$/, "");
  const route = page === "index" ? "" : page.replace(/\/index$/, "");
  return `/${route}${route ? "/" : ""}`;
};

const urls = [...new Set(Object.keys(pageFiles).map(toRoute))]
  .sort((a, b) => a.localeCompare(b))
  .map((route) => `  <url><loc>${new URL(route, site).href}</loc></url>`)
  .join("\n");

export const GET: APIRoute = () =>
  new Response(`<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`, {
    headers: {
      "Content-Type": "application/xml; charset=utf-8",
    },
  });
