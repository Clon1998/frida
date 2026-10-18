import { defineCollection } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

// Every page under src/content/docs/: the hand-written ones (landing page, quick start,
// download) and the ones scripts/sync-docs.mjs generates from the repo's docs/ folder.
export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
