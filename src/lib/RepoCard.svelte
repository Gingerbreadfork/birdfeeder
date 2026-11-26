<script module lang="ts">
  import type { Look } from "./card";

  /** The colours chosen last, kept while the app stays open. */
  let lastLook: Look | null = null;
</script>

<script lang="ts">
  import { api, reveal } from "./api";
  import { CARD, cardCanvas, cardData, drawCard, loadCardFonts } from "./card";
  import { num } from "./format";
  import { repoName } from "./kinds";
  import { android, mobile } from "./platform";
  import { watcher } from "./state.svelte";
  import { showsDark } from "./theme";
  import type { RepoDetail } from "./types";

  let { detail }: { detail: RepoDetail } = $props();

  const looks: { id: Look; label: string }[] = [
    { id: "light", label: "Light" },
    { id: "dark", label: "Dark" },
  ];

  const card = $derived(cardData(detail));
  const name = $derived(repoName(detail.repo.name, watcher.login));

  let look = $state<Look>(lastLook ?? (showsDark() ? "dark" : "light"));
  let busy = $state(false);
  /** What happened to the card last: where it was saved, or that it was copied. */
  let done = $state<{ saved: string } | "copied" | null>(null);
  let problem = $state("");

  const canCopy = !mobile && typeof ClipboardItem === "function" && Boolean(navigator.clipboard?.write);

  /** The file's name and the folder it is in, for a path with either kind of slash. */
  const place = $derived.by(() => {
    if (!done || done === "copied") return null;
    const parts = done.saved.split(/[\\/]/);
    return { path: done.saved, file: parts.at(-1) ?? "", folder: parts.at(-2) ?? "" };
  });

  /** Whether the press now under way began outside the dialog's box, on the dimmed page. */
  let pressedOutside = false;

  function close() {
    watcher.card = false;
  }

  /** Opens the dialog over everything else, which also keeps the keyboard inside it. */
  function modal(node: HTMLDialogElement) {
    node.showModal();
    node.querySelector<HTMLElement>(".primary")?.focus();
    return () => node.close();
  }

  /**
   * Draws the card once its typefaces are loaded, and again when its figures, colours or size
   * change. One canvas pixel for each pixel of the screen keeps the small copy sharp.
   */
  function paint(node: HTMLCanvasElement) {
    const [data, colours] = [card, look];
    const sized = new ResizeObserver(() => {
      const width = Math.round(node.clientWidth * devicePixelRatio);
      const ctx = width ? node.getContext("2d") : null;
      if (!ctx) return;
      node.width = width;
      node.height = Math.round((width * CARD.height) / CARD.width);
      ctx.scale(width / CARD.width, width / CARD.width);
      drawCard(ctx, data, colours);
    });
    let wanted = true;
    void loadCardFonts(data)
      .catch(() => {})
      .then(() => {
        if (wanted) sized.observe(node);
      });
    return () => {
      wanted = false;
      sized.disconnect();
    };
  }

  function pick(id: Look) {
    look = lastLook = id;
    done = null;
  }

  /** The card at full size, as a canvas. */
  async function full(): Promise<HTMLCanvasElement> {
    await loadCardFonts(card).catch(() => {});
    return cardCanvas(card, look);
  }

  async function png(): Promise<Blob> {
    const canvas = await full();
    return new Promise((resolve, reject) => {
      canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("The card could not be drawn."))), "image/png");
    });
  }

  async function save() {
    busy = true;
    problem = "";
    try {
      const picture = await png();
      done = { saved: await api.saveCard(card.name, new Uint8Array(await picture.arrayBuffer())) };
    } catch (reason) {
      problem = `The card was not saved. ${reason}`;
    } finally {
      busy = false;
    }
  }

  async function copy() {
    problem = "";
    try {
      // The promise goes in unresolved, so the copy still counts as part of the click.
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png() })]);
      done = "copied";
    } catch {
      problem = "The card could not be copied. Save it instead.";
    }
  }

  /** Hands the card to the phone's share sheet, with the repository's address beside it. */
  async function share() {
    problem = "";
    try {
      const picture = (await full()).toDataURL("image/png").split(",")[1];
      if (!android?.shareImage(card.name, picture, card.url)) problem = "The card could not be shared.";
    } catch (reason) {
      problem = `The card could not be shared. ${reason}`;
    }
  }
</script>

<dialog
  {@attach modal}
  aria-labelledby="card-title"
  onclose={close}
  onpointerdown={(e) => (pressedOutside = e.target === e.currentTarget)}
  onclick={(e) => {
    if (pressedOutside && e.target === e.currentTarget) close();
  }}
>
  <div class="sheet">
    <canvas {@attach paint}>
      A card for {name}: {num(card.stars)} stars, its star history, and a QR code that opens the repository.
    </canvas>

    <div class="side">
      <header>
        <h2 id="card-title">A card for {name}</h2>
        <button class="close" onclick={close} aria-label="Close">
          <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
            <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
        </button>
      </header>
      <p class="about">
        A picture of how it is doing, to send to anyone who should know. The code in the corner opens the
        repository.
      </p>

      <div class="segmented" role="radiogroup" aria-label="Colours">
        {#each looks as option (option.id)}
          <label class={{ on: look === option.id }}>
            <input
              type="radio"
              name="card-look"
              value={option.id}
              checked={look === option.id}
              onchange={() => pick(option.id)}
            />
            {option.label}
          </label>
        {/each}
      </div>

      <div class="foot">
        <p class="status" role="status">
          {#if problem}
            <span class="problem">{problem}</span>
          {:else if place}
            Saved as <strong class="selectable">{place.file}</strong>{place.folder ? ` in ${place.folder}.` : "."}
            <button class="link" onclick={() => reveal(place.path)}>Show in folder</button>
          {:else if done === "copied"}
            Copied. Paste it into a message.
          {/if}
        </p>
        <div class="actions">
          {#if mobile}
            <button class="btn primary" onclick={share}>Share</button>
          {:else}
            <button class="btn primary" onclick={save} disabled={busy}>Save image</button>
            {#if canCopy}
              <button class="btn" onclick={copy}>Copy image</button>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  </div>
</dialog>

<style>
  dialog {
    max-width: calc(100vw - 24px);
    max-height: calc(100dvh - 24px);
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 24px 70px #00000059;
    overflow: auto;
    animation: arrive 0.2s cubic-bezier(0.2, 0.9, 0.3, 1) both;
  }

  dialog::backdrop {
    background: #0b130ea8;
  }

  .sheet {
    display: flex;
    align-items: stretch;
    gap: 22px;
    padding: 18px;
  }

  /* The card keeps its shape and takes as much height as the window can spare. */
  canvas {
    display: block;
    flex: none;
    height: min(620px, calc(100dvh - 62px));
    aspect-ratio: 4 / 5;
    border-radius: 10px;
    box-shadow:
      0 0 0 1px var(--line),
      0 8px 24px #00000024;
  }

  .side {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    width: 268px;
    padding: 4px 2px 2px 0;
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
  }

  h2 {
    font-size: var(--fs-lg);
    line-height: 1.25;
    overflow-wrap: anywhere;
  }

  .close {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    margin: -3px -4px 0 0;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--ink-3);
  }

  @media (hover: hover) {
    .close:hover {
      background: var(--accent-wash);
      color: var(--ink);
    }
  }

  .about {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  .foot {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    margin-top: auto;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .status {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }

  .status strong {
    color: var(--ink);
    font-weight: 600;
  }

  .problem {
    color: var(--danger);
  }

  .status:empty {
    display: none;
  }

  .link {
    display: block;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 550;
  }

  @media (hover: hover) {
    .link:hover {
      text-decoration: underline;
    }
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.985);
    }
  }

  /* Too narrow for two columns: the card goes on top and shrinks to leave room for the rest. */
  @media (max-width: 760px) {
    .sheet {
      flex-direction: column;
      align-items: center;
      gap: 16px;
      padding: 14px;
    }

    canvas {
      width: min(100%, calc((100dvh - 300px) * 0.8));
      min-width: 200px;
      height: auto;
    }

    .side {
      width: 100%;
      max-width: 420px;
      padding: 0;
    }

    .segmented {
      display: flex;
      width: 100%;
    }

    .segmented label {
      flex: 1;
    }

    .actions .btn {
      flex: 1;
      justify-content: center;
    }
  }
</style>
