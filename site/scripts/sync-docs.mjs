// Turns the Markdown in `docs/` into Starlight pages under src/content/docs/ (git-ignored),
// and copies the logo. Runs before `astro dev` / `astro build`.
//
// Per file: the first `# Heading` becomes the `title` front matter, the first paragraph the
// `description`, and relative links are rewritten: to other docs → their page on this site,
// to anything else in the repo (PLAN.md, source files) → GitHub.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { BASE, DOCS, REPO } from "../site.config.mjs";

const SITE_DIR = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const REPO_DIR = path.dirname(SITE_DIR);
const OUT_DIR = path.join(SITE_DIR, "src/content/docs");

const pageFor = new Map(DOCS.map((d) => [`docs/${d.file}`, d.slug]));

/** Rewrites one link target found in `docs/<file>`. */
function rewriteLink(target) {
  if (/^([a-z]+:|#|\/)/i.test(target)) return target;
  const [file, hash = ""] = target.split("#");
  const repoPath = path.posix.normalize(path.posix.join("docs", file));
  const anchor = hash ? `#${hash}` : "";
  const slug = pageFor.get(repoPath);
  if (slug) return `${BASE}/${slug}/${anchor}`;
  if (repoPath.startsWith("..")) throw new Error(`link leaves the repository: ${target}`);
  const isDir = fs.statSync(path.join(REPO_DIR, repoPath), { throwIfNoEntry: false })?.isDirectory();
  return `${REPO}/${isDir ? "tree" : "blob"}/main/${repoPath}${anchor}`;
}

/** Plain text of a Markdown paragraph, for the page description. */
const plainText = (markdown) =>
  markdown
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_`]/g, "")
    .replace(/\s+/g, " ")
    .trim();

function convert({ file, slug }) {
  const source = fs.readFileSync(path.join(REPO_DIR, "docs", file), "utf8");
  const lines = source.split("\n");
  const h1 = lines.findIndex((l) => l.startsWith("# "));
  if (h1 < 0) throw new Error(`docs/${file} has no "# Title" line`);
  const title = lines[h1].slice(2).trim();
  let body = lines.slice(h1 + 1).join("\n");

  // The intro paragraph (if the page starts with one rather than a heading).
  const intro = body.trimStart().split(/\n\s*\n/)[0];
  const description = intro.startsWith("#") ? "" : plainText(intro);

  // `[text](target)`, outside of code: code spans/blocks are kept as they are.
  const parts = body.split(/(```[\s\S]*?```|`[^`\n]*`)/);
  body = parts
    .map((part, i) =>
      i % 2 === 1 ? part : part.replace(/\]\(([^)\s]+)\)/g, (_, t) => `](${rewriteLink(t)})`),
    )
    .join("");

  // JSON strings are valid YAML, which saves escaping by hand.
  const frontMatter = [
    "---",
    `title: ${JSON.stringify(title)}`,
    description && `description: ${JSON.stringify(description)}`,
    `editUrl: ${JSON.stringify(`${REPO}/edit/main/docs/${file}`)}`,
    "---",
    // .md, not .mdx: `{{variables}}` in the text must not be read as JSX expressions.
    `<!-- Generated from docs/${file} by scripts/sync-docs.mjs. Edit that file instead. -->`,
  ].filter(Boolean);

  const out = path.join(OUT_DIR, `${slug}.md`);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, `${frontMatter.join("\n")}\n${body}`);
}

// Start clean, so a doc removed from DOCS doesn't linger as a page.
for (const d of DOCS) fs.rmSync(path.join(OUT_DIR, `${d.slug}.md`), { force: true });
for (const dir of ["guides", "contributing"]) fs.rmSync(path.join(OUT_DIR, dir), { recursive: true, force: true });
DOCS.forEach(convert);

// The app icon is the logo and favicon; assets/ stays its only copy in git.
fs.copyFileSync(path.join(REPO_DIR, "assets/app-icon.svg"), path.join(SITE_DIR, "public/favicon.svg"));
fs.copyFileSync(path.join(REPO_DIR, "assets/app-icon.svg"), path.join(SITE_DIR, "src/assets/logo.svg"));

console.log(`sync-docs: ${DOCS.length} pages from docs/`);
