<script lang="ts">
  export type IconName =
    | "home"
    | "file"
    | "settings"
    | "plus"
    | "minus"
    | "maximize"
    | "restore"
    | "close"
    | "search"
    | "chevron-up"
    | "chevron-down"
    | "fit-width"
    | "fit-page"
    | "dual-page"
    | "rotate-left"
    | "rotate-right"
    | "highlight"
    | "underline"
    | "strikeout"
    | "note"
    | "ink"
    | "comments"
    | "outline"
    | "signature"
    | "save";

  interface Props {
    name: IconName;
    size?: number;
    strokeWidth?: number;
  }

  let { name, size = 18, strokeWidth = 1.8 }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "home"}
    <path d="m3.5 10.5 8.5-7 8.5 7" />
    <path d="M5.5 9.5V21h13V9.5M9.5 21v-6h5v6" />
  {:else if name === "file"}
    <path d="M6 2.75h8l4 4V21.25H6z" />
    <path d="M14 2.75v4h4M9 11h6M9 15h6" />
  {:else if name === "settings"}
    <circle cx="12" cy="12" r="3" />
    <path d="M19.2 13.8a7.4 7.4 0 0 0 0-3.6l2-1.5-2-3.4-2.4 1a7.7 7.7 0 0 0-3.1-1.8L13.4 2h-4l-.3 2.5A7.7 7.7 0 0 0 6 6.3l-2.4-1-2 3.4 2 1.5a7.4 7.4 0 0 0 0 3.6l-2 1.5 2 3.4 2.4-1a7.7 7.7 0 0 0 3.1 1.8l.3 2.5h4l.3-2.5a7.7 7.7 0 0 0 3.1-1.8l2.4 1 2-3.4z" />
  {:else if name === "plus"}
    <path d="M12 5v14M5 12h14" />
  {:else if name === "minus"}
    <path d="M5 12h14" />
  {:else if name === "maximize"}
    <rect x="6.5" y="6.5" width="11" height="11" />
  {:else if name === "restore"}
    <path d="M8.5 8.5v-3h10v10h-3" />
    <rect x="5.5" y="8.5" width="10" height="10" />
  {:else if name === "close"}
    <path d="m6 6 12 12M18 6 6 18" />
  {:else if name === "search"}
    <circle cx="10.5" cy="10.5" r="6.5" />
    <path d="m15.5 15.5 5 5" />
  {:else if name === "chevron-up"}
    <path d="m7 14 5-5 5 5" />
  {:else if name === "chevron-down"}
    <path d="m7 10 5 5 5-5" />
  {:else if name === "fit-width"}
    <rect x="5" y="4" width="14" height="16" rx="1.5" />
    <path d="M2 12h7M2 12l2.5-2.5M2 12l2.5 2.5M22 12h-7M22 12l-2.5-2.5M22 12l-2.5 2.5" />
  {:else if name === "fit-page"}
    <path d="M8 3H4a1 1 0 0 0-1 1v4M16 3h4a1 1 0 0 1 1 1v4M8 21H4a1 1 0 0 1-1-1v-4M16 21h4a1 1 0 0 0 1-1v-4" />
    <rect x="7" y="5.5" width="10" height="13" rx="1" />
  {:else if name === "dual-page"}
    <rect x="2.5" y="4.5" width="8.5" height="15" rx="1" />
    <rect x="13" y="4.5" width="8.5" height="15" rx="1" />
  {:else if name === "rotate-left"}
    <path d="M4 4v6h6" />
    <path d="M5.5 17.5A8 8 0 1 0 4.2 10" />
  {:else if name === "rotate-right"}
    <path d="M20 4v6h-6" />
    <path d="M18.5 17.5A8 8 0 1 1 19.8 10" />
  {:else if name === "highlight"}
    <path d="m15.5 3.5 5 5L10 19H5v-5z" />
    <path d="m12.5 6.5 5 5M4 21h16" />
  {:else if name === "underline"}
    <path d="M7 4v7a5 5 0 0 0 10 0V4M5 21h14" />
  {:else if name === "strikeout"}
    <path d="M17.5 6.5C16.3 4.8 14.5 4 12 4c-3 0-5 1.5-5 3.7 0 4.8 10 2.2 10 7.6 0 2.5-2 4.2-5.4 4.2-2.6 0-4.6-.9-5.8-2.8" />
    <path d="M3.5 12h17" />
  {:else if name === "note"}
    <path d="M5 3.5h14v12l-5.5 5H5z" />
    <path d="M13.5 20.5v-5h5M8 8h8M8 11.5h5" />
  {:else if name === "ink"}
    <path d="m4 17 11.8-11.8a2.1 2.1 0 0 1 3 3L7 20H4z" />
    <path d="m14.5 6.5 3 3M3 21h18" />
  {:else if name === "comments"}
    <path d="M4 4.5h16v12H9l-5 4z" />
    <path d="M8 9h8M8 12h5" />
  {:else if name === "outline"}
    <path d="M9 6h11M9 12h11M9 18h11" />
    <path d="M4 6h.01M4 12h.01M4 18h.01" stroke-width="3" />
  {:else if name === "signature"}
    <path d="M3 17c3.2-6.8 5.5-10.2 7-10.2 2.4 0-1.2 9.2.8 9.2 1.3 0 2.2-3 3.4-3 1 0 .8 3 2 3 1.1 0 1.7-1.8 2.7-1.8.7 0 1.1.7 1.1 1.6" />
    <path d="M3 21h18" />
  {:else if name === "save"}
    <path d="M4 3h13l3 3v15H4z" />
    <path d="M8 3v6h8V3M8 21v-7h8v7" />
  {/if}
</svg>
