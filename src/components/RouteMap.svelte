<script lang="ts">
  import type { LatLng } from "../lib/api";

  let {
    points,
    closed,
    position = null,
    lapProgress = 0,
    running = false,
    dim = false,
  }: {
    /** [lat, lng] pairs, WGS-84 */
    points: [number, number][];
    closed: boolean;
    position?: LatLng | null;
    lapProgress?: number;
    running?: boolean;
    dim?: boolean;
  } = $props();

  let w = $state(0);
  let h = $state(0);
  const PAD = 36;
  const M_PER_DEG = 111_195;

  // Planar projection (meters) around the route's centre.
  const geo = $derived.by(() => {
    if (points.length < 2) return null;
    let minLat = Infinity, maxLat = -Infinity, minLng = Infinity, maxLng = -Infinity;
    for (const [la, ln] of points) {
      minLat = Math.min(minLat, la); maxLat = Math.max(maxLat, la);
      minLng = Math.min(minLng, ln); maxLng = Math.max(maxLng, ln);
    }
    const lat0 = (minLat + maxLat) / 2;
    const lng0 = (minLng + maxLng) / 2;
    const kx = M_PER_DEG * Math.cos((lat0 * Math.PI) / 180);
    const toM = (la: number, ln: number) => [(ln - lng0) * kx, (la - lat0) * M_PER_DEG] as const;
    const spanX = Math.max(1, (maxLng - minLng) * kx);
    const spanY = Math.max(1, (maxLat - minLat) * M_PER_DEG);
    return { toM, spanX, spanY };
  });

  const view = $derived.by(() => {
    if (!geo || w < 50 || h < 50) return null;
    const scale = Math.min((w - PAD * 2) / geo.spanX, (h - PAD * 2) / geo.spanY);
    const project = (la: number, ln: number) => {
      const [x, y] = geo.toM(la, ln);
      return [w / 2 + x * scale, h / 2 - y * scale] as const;
    };
    const pts = points.map(([la, ln]) => project(la, ln));
    if (closed) pts.push(pts[0]);
    const d = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join("");
    // Scale bar: pick a round length ≈ 18 % of the width.
    const target = (w * 0.18) / scale;
    const nice = [10, 20, 50, 100, 200, 250, 500, 1000, 2000, 5000].find((n) => n >= target) ?? 5000;
    // Direction hint: a short arrow just after the start.
    const [sx, sy] = pts[0];
    let k = 1;
    while (k < pts.length - 1 && Math.hypot(pts[k][0] - sx, pts[k][1] - sy) < 18) k++;
    const ang = (Math.atan2(pts[k][1] - sy, pts[k][0] - sx) * 180) / Math.PI;
    return { project, d, start: pts[0], end: pts[pts.length - 1], barPx: nice * scale, barM: nice, ang };
  });

  const dot = $derived(view && position ? view.project(position.lat, position.lng) : null);
  // For out-and-back laps the drawn line is traversed twice.
  const drawnProgress = $derived(closed ? lapProgress : lapProgress < 0.5 ? lapProgress * 2 : 1);
</script>

<div class="map" class:dim bind:clientWidth={w} bind:clientHeight={h}>
  {#if view}
    <svg width={w} height={h} viewBox="0 0 {w} {h}" role="img" aria-label="路线示意图">
      <defs>
        <pattern id="grid" width="28" height="28" patternUnits="userSpaceOnUse">
          <path d="M28 0H0V28" fill="none" stroke="var(--map-grid)" stroke-width="1" />
        </pattern>
      </defs>
      <rect width={w} height={h} fill="url(#grid)" />

      <path class="route-casing" d={view.d} />
      <path class="route" d={view.d} />
      {#if running || lapProgress > 0}
        <path
          class="done"
          d={view.d}
          pathLength="1000"
          stroke-dasharray="{(drawnProgress * 1000).toFixed(1)} 1000"
        />
      {/if}

      {#if !closed}
        <circle class="end" cx={view.end[0]} cy={view.end[1]} r="5" />
      {/if}
      <g transform="translate({view.start[0]},{view.start[1]})">
        <circle class="start" r="8" />
        <path class="arrow" d="M-3.5,-4 L4,0 L-3.5,4 Z" transform="rotate({view.ang})" />
      </g>

      {#if dot}
        <g transform="translate({dot[0]},{dot[1]})" class="runner" class:running>
          <circle class="halo" r="16" />
          <circle class="ring" r="8" />
          <circle class="core" r="5.5" />
        </g>
      {/if}

      <g transform="translate(18,{h - 22})" class="scale">
        <path d="M0,-5V0H{view.barPx}V-5" />
        <text x={view.barPx + 8} y="1">{view.barM >= 1000 ? `${view.barM / 1000} km` : `${view.barM} m`}</text>
      </g>
      <g transform="translate({w - 26},26)" class="north">
        <path d="M0,-11 L5,4 L0,1 L-5,4 Z" />
        <text y="17">N</text>
      </g>
    </svg>
  {:else}
    <div class="empty">选择或导入一条路线</div>
  {/if}
</div>

<style>
  .map {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--map-bg);
    overflow: hidden;
  }
  svg {
    display: block;
  }
  .route-casing {
    fill: none;
    stroke: var(--panel);
    stroke-width: 11;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .route {
    fill: none;
    stroke: var(--route);
    stroke-width: 5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .done {
    fill: none;
    stroke: var(--accent);
    stroke-width: 5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .dim .route,
  .dim .done {
    opacity: 0.55;
  }
  .start {
    fill: var(--panel);
    stroke: var(--accent);
    stroke-width: 2.5;
  }
  .arrow {
    fill: var(--accent);
  }
  .end {
    fill: var(--panel);
    stroke: var(--fg-2);
    stroke-width: 2;
  }
  .runner .halo {
    fill: var(--runner-soft);
    opacity: 0;
  }
  .runner.running .halo {
    transform-box: fill-box;
    transform-origin: center;
    animation: pulse 1.6s ease-out infinite;
  }
  .runner .ring {
    fill: #fff;
    filter: drop-shadow(0 1px 2px rgb(0 0 0 / 0.35));
  }
  .runner .core {
    fill: var(--runner);
  }
  @keyframes pulse {
    0% {
      opacity: 0.9;
      transform: scale(0.4);
    }
    100% {
      opacity: 0;
      transform: scale(1.3);
    }
  }
  .scale path {
    fill: none;
    stroke: var(--fg-2);
    stroke-width: 1.5;
  }
  .scale text,
  .north text {
    font-size: 11px;
    fill: var(--fg-2);
    dominant-baseline: middle;
  }
  .north path {
    fill: var(--fg-2);
  }
  .north text {
    text-anchor: middle;
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--muted);
  }
</style>
