<script lang="ts">
  let {
    size = 48,
    perch = true,
  }: {
    size?: number;
    /** Whether the bird stands on its perch. Without it the bird works as a small mark. */
    perch?: boolean;
  } = $props();
</script>

<svg
  viewBox={perch ? "180 310 656 494" : "180 318 656 404"}
  width={size}
  height={size * (perch ? 0.753 : 0.616)}
  aria-hidden="true"
>
  {#if perch}
    <rect class="perch" x="204" y="748" width="616" height="48" rx="24" />
    <g class="legs">
      <line x1="458" y1="700" x2="458" y2="752" />
      <line x1="556" y1="700" x2="556" y2="752" />
    </g>
  {/if}
  <path class="tail" d="M348 500 L348 592 L214 470 L262 400 Z" />
  <rect class="body" x="318" y="326" width="388" height="388" rx="110" />
  <path class="dark" d="M572 500 C486 486 402 532 366 618 C446 660 552 650 580 574 C590 546 586 520 572 500 Z" />
  <path class="beak" d="M702 442 L792 478 L702 514 Z" />
  <circle class="dark" cx="614" cy="450" r="26" />
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }

  .perch {
    fill: var(--line-strong);
  }

  .legs {
    stroke: #ee8140;
    stroke-width: 22;
    stroke-linecap: round;
  }

  .tail {
    fill: #d2921c;
    stroke: #d2921c;
    stroke-width: 44;
    stroke-linejoin: round;
  }

  .body {
    fill: #f4bc3e;
  }

  .dark {
    fill: #13241b;
  }

  .beak {
    fill: #ee8140;
    stroke: #ee8140;
    stroke-width: 18;
    stroke-linejoin: round;
  }
</style>
