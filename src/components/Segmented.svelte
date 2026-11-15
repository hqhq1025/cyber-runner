<script lang="ts" generics="T extends string">
  // Radio-group based segmented control: native keyboard behaviour (arrow
  // keys), one tab stop, works with screen readers.
  let {
    label,
    options,
    value,
    onchange,
    disabled = false,
    size = "md",
  }: {
    label: string;
    options: { value: T; label: string; title?: string }[];
    value: T;
    onchange: (v: T) => void;
    disabled?: boolean;
    size?: "sm" | "md";
  } = $props();

  const name = `seg-${Math.random().toString(36).slice(2, 9)}`;
</script>

<div class="seg {size}" role="radiogroup" aria-label={label} class:disabled>
  {#each options as o (o.value)}
    <label class:checked={o.value === value} title={o.title}>
      <input
        type="radio"
        {name}
        value={o.value}
        checked={o.value === value}
        {disabled}
        onchange={() => onchange(o.value)}
      />
      <span>{o.label}</span>
    </label>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--panel-2);
    border: 1px solid var(--line);
  }
  label {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    padding: 0 8px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    color: var(--fg-2);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
  }
  .sm label {
    height: 24px;
    font-size: 12.5px;
  }
  label:hover {
    color: var(--fg);
  }
  label.checked {
    background: var(--panel);
    color: var(--fg);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12), 0 0 0 1px var(--line);
  }
  input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: inherit;
  }
  label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .disabled label {
    cursor: not-allowed;
    opacity: 0.55;
  }
</style>
