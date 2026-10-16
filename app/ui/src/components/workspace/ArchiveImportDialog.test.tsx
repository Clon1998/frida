import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ArchiveSummary } from "@/bindings";
import { useDialogs } from "@/stores/dialogs";
import { useWorkspace } from "@/stores/workspace";
import { ArchiveImportDialog } from "./ArchiveImportDialog";

const summary = (kind: ArchiveSummary["manifest"]["kind"]): ArchiveSummary => ({
  manifest: {
    format: 1,
    kind,
    name: "Orders",
    fridaVersion: "0.1.0",
    exportedAt: "2026-10-16T10:00:00Z",
  },
  files: 4,
  warnings: [],
});

describe("archive import dialog", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    useDialogs.setState({ archiveImport: null });
  });

  it("previews a folder archive and imports it into the chosen folder", async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "plugin:dialog|open") return "/tmp/orders.frida.zip";
      if (cmd === "preview_archive") return summary("folder");
      if (cmd === "import_archive")
        return {
          imported: { summary: summary("folder"), dir: "/col", path: "api/orders" },
          overview: null,
        };
      return [];
    });
    useWorkspace.setState({ workspace: { dir: "/ws", name: "WS", collections: [] } });
    useDialogs.getState().openArchiveImport("/col", "api");
    render(<ArchiveImportDialog />);

    fireEvent.click(screen.getByText("Choose archive…"));
    await screen.findByText(/with 4 files/);
    expect(screen.getByText(/into the folder "api"/)).toBeTruthy();

    fireEvent.click(screen.getByText("Import"));
    await waitFor(() => expect(useDialogs.getState().archiveImport).toBeNull());
    expect(calls).toContainEqual([
      "import_archive",
      {
        file: "/tmp/orders.frida.zip",
        target: { kind: "into", collectionDir: "/col", parent: "api" },
      },
    ]);
  });

  it("asks for a collection before importing a request without one", async () => {
    mockIPC((cmd) => {
      if (cmd === "plugin:dialog|open") return "/tmp/r.zip";
      if (cmd === "preview_archive") return summary("request");
      return null;
    });
    useDialogs.getState().openArchiveImport(null, "");
    render(<ArchiveImportDialog />);
    fireEvent.click(screen.getByText("Choose archive…"));
    await screen.findByText("Open a collection first to import into it.");
    expect((screen.getByText("Import") as HTMLButtonElement).disabled).toBe(true);
  });
});
