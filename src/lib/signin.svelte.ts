import { api, open } from "./api";
import type { SignInCode } from "./types";

/** How long "Signed in" stays on screen before the next sign-in starts afresh. */
const SHOW_DONE = 4000;

/**
 * Signing in through GitHub, kept outside any one screen, since the screen showing it can be
 * drawn again while the user is away in the browser.
 */
class SignInFlow {
  code = $state<SignInCode | null>(null);
  asking = $state(false);
  copied = $state(false);
  error = $state<string | null>(null);
  signedIn = $state<string | null>(null);
  /** Counts attempts, so an answer for one that was cancelled is ignored. */
  private attempt = 0;

  async start(onsigned?: (login: string) => void) {
    const mine = ++this.attempt;
    this.asking = true;
    this.error = null;
    this.copied = false;
    this.signedIn = null;
    try {
      this.code = await api.signInStart();
    } catch (reason) {
      this.error = String(reason);
      return;
    } finally {
      this.asking = false;
    }
    try {
      const login = await api.signInWait();
      if (mine !== this.attempt) return;
      this.code = null;
      this.signedIn = login;
      setTimeout(() => {
        if (mine === this.attempt) this.signedIn = null;
      }, SHOW_DONE);
      onsigned?.(login);
    } catch (reason) {
      if (mine !== this.attempt) return;
      this.code = null;
      this.error = String(reason);
    }
  }

  cancel() {
    this.attempt++;
    this.code = null;
    void api.signInCancel();
  }

  /** Copies the code while the tap still counts as one, then hands over to the browser. */
  copyAndOpen() {
    const code = this.code;
    if (!code) return;
    navigator.clipboard?.writeText(code.user_code).then(
      () => (this.copied = true),
      () => {},
    );
    open(code.verification_uri);
  }
}

export const signIn = new SignInFlow();
