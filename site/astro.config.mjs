// @ts-check
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import { BASE, DOCS, REPO, SITE } from "./site.config.mjs";

/** Sidebar entry for a page generated from `docs/` (see site.config.mjs). */
const doc = (/** @type {string} */ slug) => {
  const entry = DOCS.find((d) => d.slug === slug);
  if (!entry) throw new Error(`unknown doc page: ${slug}`);
  return { label: entry.label, slug };
};

export default defineConfig({
  site: SITE,
  base: BASE,
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "Frida",
      description:
        "A local-first desktop HTTP client. Your requests are plain YAML files in your git repo. No account, no cloud, no telemetry.",
      logo: { src: "./src/assets/logo.svg", alt: "Frida logo" },
      favicon: "/favicon.svg",
      // The preview picture for links shared in Slack, LinkedIn, X… (app/ui: pnpm screenshots).
      head: [
        { tag: "meta", attrs: { property: "og:image", content: `${SITE}${BASE}/og.png` } },
        { tag: "meta", attrs: { name: "twitter:image", content: `${SITE}${BASE}/og.png` } },
        { tag: "meta", attrs: { property: "og:image:alt", content: "Frida: your API requests, as files in your repo." } },
      ],
      social: [{ icon: "github", label: "GitHub", href: REPO }],
      editLink: { baseUrl: `${REPO}/edit/main/site/` },
      lastUpdated: true,
      customCss: ["./src/styles/custom.css"],
      // The landing page's hero, see the comment in the component.
      components: { Hero: "./src/components/Hero.astro" },
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Quick start", slug: "quick-start" },
            { label: "Download", slug: "download" },
            doc("migrating"),
          ],
        },
        {
          label: "Guides",
          items: [
            doc("guides/user-guide"),
            doc("guides/teams"),
            doc("guides/scripting"),
            doc("guides/cli"),
            doc("guides/file-format"),
          ],
        },
        {
          label: "Contributing",
          collapsed: true,
          items: [
            doc("contributing/architecture"),
            doc("contributing/development"),
            doc("contributing/releasing"),
          ],
        },
        {
          label: "About",
          items: [doc("security"), doc("faq"), doc("changelog")],
        },
      ],
    }),
  ],
});
