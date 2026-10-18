import { REPO } from "../../site.config.mjs";

/** One file to download, e.g. the Apple silicon .dmg. */
export interface Download {
  label: string;
  /** Short hint shown next to the label, e.g. "Apple silicon". */
  detail: string;
  url: string;
  sizeMb: number | null;
}

export interface Platform {
  id: "macos" | "windows" | "linux";
  name: string;
  /** The app's installers; the first one is the "Download" button's default. */
  app: Download[];
  /** The `frida-run` CLI archive(s). */
  cli: Download[];
  note?: string;
}

export interface Release {
  version: string | null;
  date: string | null;
  /** The release page; also the fallback link when the GitHub API couldn't be reached. */
  pageUrl: string;
  platforms: Platform[];
}

interface GitHubAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

/**
 * Which release file goes where. Matched against the asset names the release workflow
 * produces (`Frida_0.3.0_aarch64.dmg`, `frida-run-x86_64-unknown-linux-gnu.tar.gz`, …).
 */
const LAYOUT: {
  id: Platform["id"];
  name: string;
  note?: string;
  app: [RegExp, string, string][];
  cli: [RegExp, string, string][];
}[] = [
  {
    id: "macos",
    name: "macOS",
    note: "Signed and notarized by Apple.",
    app: [
      [/_aarch64\.dmg$/, "Download .dmg", "Apple silicon"],
      [/_x64\.dmg$/, "Download .dmg", "Intel"],
    ],
    cli: [
      [/^frida-run-aarch64-apple-darwin\.tar\.gz$/, "frida-run", "Apple silicon"],
      [/^frida-run-x86_64-apple-darwin\.tar\.gz$/, "frida-run", "Intel"],
    ],
  },
  {
    id: "windows",
    name: "Windows",
    note: "Not code-signed yet: SmartScreen asks once (More info → Run anyway).",
    app: [
      [/_x64-setup\.exe$/, "Download installer", ".exe, x64"],
      [/_x64_en-US\.msi$/, "Download .msi", "x64"],
    ],
    cli: [[/^frida-run-x86_64-pc-windows-msvc\.zip$/, "frida-run", "x64"]],
  },
  {
    id: "linux",
    name: "Linux",
    note: "AppImage: chmod +x, then run it.",
    app: [
      [/_amd64\.AppImage$/, "Download .AppImage", "x64"],
      [/_amd64\.deb$/, "Download .deb", "Debian, Ubuntu"],
      [/\.x86_64\.rpm$/, "Download .rpm", "Fedora, openSUSE"],
    ],
    cli: [[/^frida-run-x86_64-unknown-linux-gnu\.tar\.gz$/, "frida-run", "x64"]],
  },
];

let cached: Promise<Release> | undefined;

/**
 * The latest GitHub release, fetched once per build. The site is static, so new releases show
 * up when the site is rebuilt (the release workflow triggers that). If GitHub can't be reached
 * (offline, rate limit), every button links to the releases page instead of failing the build.
 */
export function latestRelease(): Promise<Release> {
  cached ??= load();
  return cached;
}

async function load(): Promise<Release> {
  const pageUrl = `${REPO}/releases/latest`;
  const api = REPO.replace("https://github.com/", "https://api.github.com/repos/");
  let assets: GitHubAsset[] = [];
  let version: string | null = null;
  let date: string | null = null;
  try {
    const headers: Record<string, string> = { Accept: "application/vnd.github+json" };
    // In GitHub Actions the token lifts the API's rate limit for anonymous calls.
    if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
    const response = await fetch(`${api}/releases/latest`, { headers });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const json = (await response.json()) as {
      tag_name: string;
      published_at: string;
      assets: GitHubAsset[];
    };
    assets = json.assets;
    version = json.tag_name.replace(/^v/, "");
    date = json.published_at;
  } catch (err) {
    console.warn(`[release] latest release not available, linking the releases page: ${err}`);
  }

  const pick = ([pattern, label, detail]: [RegExp, string, string]): Download => {
    const asset = assets.find((a) => pattern.test(a.name));
    return {
      label,
      detail,
      url: asset?.browser_download_url ?? pageUrl,
      sizeMb: asset ? Math.round(asset.size / 1_000_000) : null,
    };
  };
  return {
    version,
    date,
    pageUrl,
    platforms: LAYOUT.map(({ id, name, note, app, cli }) => ({
      id,
      name,
      note,
      app: app.map(pick),
      cli: cli.map(pick),
    })),
  };
}
