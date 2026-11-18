import { BundleType, getBundleType } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { api } from "./api";
import { mobile } from "./platform";

export interface UpdateHandle {
  version: string;
  notes: string;
  /**
   * Downloads and installs it, reporting download progress from 0 to 1.
   * Missing on a phone, where the new package is fetched from the release page.
   */
  install?(onProgress: (fraction: number) => void): Promise<void>;
}

/** Where a release's packages can be downloaded by hand. */
export const RELEASE_PAGE = "https://github.com/Gingerbreadfork/birdfeeder/releases/tag/v";

/** The newer release, if there is one. */
export async function checkForUpdate(): Promise<UpdateHandle | null> {
  if (mobile) return api.latestRelease();
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
  if (mobile) return false;
  try {
    const type = await getBundleType();
    return type === BundleType.Deb || type === BundleType.Rpm;
  } catch {
    return false;
  }
}

export { relaunch };
