import { BundleType, getBundleType } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export interface UpdateHandle {
  version: string;
  notes: string;
  /** Downloads and installs it, reporting download progress from 0 to 1. */
  install(onProgress: (fraction: number) => void): Promise<void>;
}

/** The newer release, if there is one. */
export async function checkForUpdate(): Promise<UpdateHandle | null> {
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? "",
    async install(onProgress) {
      let total = 0;
      let got = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          got += event.data.chunkLength;
          if (total > 0) onProgress(Math.min(1, got / total));
        } else if (event.event === "Finished") {
          onProgress(1);
        }
      });
    },
  };
}

/** Whether an update goes through the package manager, which asks for a password. */
export async function asksForPassword(): Promise<boolean> {
  try {
    const type = await getBundleType();
    return type === BundleType.Deb || type === BundleType.Rpm;
  } catch {
    return false;
  }
}

export { relaunch };
