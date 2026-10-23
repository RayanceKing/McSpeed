<script setup lang="ts">
import { computed } from "vue";
import type { TrafficBucket } from "../types";

const props = defineProps<{
  buckets: TrafficBucket[];
  /** 图表高度（px） */
  height?: number;
}>();

const WIDTH = 640;
const PADDING = { top: 10, right: 8, bottom: 20, left: 44 };

const chartHeight = computed(() => props.height ?? 140);

const plotWidth = computed(
  () => WIDTH - PADDING.left - PADDING.right
);
const plotHeight = computed(
  () => chartHeight.value - PADDING.top - PADDING.bottom
);

/** 纵轴上限：取最大值的 1.2 倍，避免折线贴顶。 */
const maxBytes = computed(() => {
  const max = props.buckets.reduce((m, b) => Math.max(m, b.bytes), 0);
  return max > 0 ? max * 1.2 : 1024;
});

/** 折线点坐标。 */
const points = computed(() =>
  props.buckets.map((b, i) => {
    const x =
      PADDING.left +
      (props.buckets.length === 1
        ? plotWidth.value / 2
        : (i / (props.buckets.length - 1)) * plotWidth.value);
    const y =
      PADDING.top +
      plotHeight.value -
      (b.bytes / maxBytes.value) * plotHeight.value;
    return { x, y, bucket: b };
  })
);

const linePath = computed(() =>
  points.value.map((p, i) => `${i === 0 ? "M" : "L"}${p.x} ${p.y}`).join(" ")
);

const areaPath = computed(() => {
  if (points.value.length === 0) return "";
  const first = points.value[0];
  const last = points.value[points.value.length - 1];
  const base = PADDING.top + plotHeight.value;
  return `M${first.x} ${base} ${linePath.value.slice(1)} L${last.x} ${base} Z`;
});

/** 纵轴刻度，四条线。 */
const gridLines = computed(() =>
  [0, 1, 2, 3].map((i) => {
    const ratio = i / 3;
    const y = PADDING.top + plotHeight.value - ratio * plotHeight.value;
    return { y, value: Math.round(maxBytes.value * ratio) };
  })
);

/** x 轴时间标签，只标首、中、末三个点。 */
const xLabels = computed(() => {
  const n = props.buckets.length;
  if (n === 0) return [];
  const indices = n <= 3 ? [...Array(n).keys()] : [0, Math.floor(n / 2), n - 1];
  return indices.map((i) => ({
    x:
      PADDING.left +
      (n === 1 ? plotWidth.value / 2 : (i / (n - 1)) * plotWidth.value),
    secondsAgo: (n - 1 - i) * 30,
  }));
});

function formatBytesShort(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)}K`;
  return `${(bytes / 1024 / 1024).toFixed(1)}M`;
}

function formatAgo(secondsAgo: number): string {
  if (secondsAgo <= 0) return "现在";
  return `-${Math.round(secondsAgo / 60)}分`;
}
</script>

<template>
  <div class="chart">
    <svg
      :viewBox="`0 0 ${WIDTH} ${chartHeight}`"
      :width="WIDTH"
      :height="chartHeight"
      preserveAspectRatio="none"
      role="img"
      aria-label="下载流量趋势"
    >
      <!-- 横向网格线与刻度 -->
      <g class="grid">
        <line
          v-for="(g, i) in gridLines"
          :key="`grid-${i}`"
          :x1="PADDING.left"
          :y1="g.y"
          :x2="WIDTH - PADDING.right"
          :y2="g.y"
        />
        <text
          v-for="(g, i) in gridLines"
          :key="`label-${i}`"
          :x="PADDING.left - 6"
          :y="g.y + 3"
          text-anchor="end"
        >
          {{ formatBytesShort(g.value) }}
        </text>
      </g>

      <!-- 面积 -->
      <path v-if="areaPath" :d="areaPath" class="area" />

      <!-- 折线 -->
      <path v-if="linePath" :d="linePath" class="line" />

      <!-- 数据点 -->
      <circle
        v-for="(p, i) in points"
        :key="`dot-${i}`"
        :cx="p.x"
        :cy="p.y"
        :r="p.bucket.bytes > 0 ? 2.5 : 0"
        class="dot"
      />

      <!-- x 轴 -->
      <line
        :x1="PADDING.left"
        :y1="PADDING.top + plotHeight"
        :x2="WIDTH - PADDING.right"
        :y2="PADDING.top + plotHeight"
        class="axis"
      />
      <text
        v-for="(l, i) in xLabels"
        :key="`x-${i}`"
        :x="l.x"
        :y="chartHeight - 5"
        text-anchor="middle"
        class="xlabel"
      >
        {{ formatAgo(l.secondsAgo) }}
      </text>
    </svg>

    <p v-if="buckets.every((b) => b.bytes === 0)" class="empty">
      最近 10 分钟没有流量
    </p>
  </div>
</template>

<style scoped>
.chart {
  position: relative;
  width: 100%;
}

svg {
  display: block;
  width: 100%;
  height: auto;
}

.grid line {
  stroke: rgba(255, 255, 255, 0.08);
  stroke-width: 1;
}

.grid text {
  fill: rgba(255, 255, 255, 0.45);
  font-size: 9px;
  font-family: Consolas, monospace;
}

.area {
  fill: rgba(60, 133, 39, 0.18);
  stroke: none;
}

.line {
  fill: none;
  stroke: #3c8527;
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.dot {
  fill: #7fd06a;
}

.axis {
  stroke: rgba(255, 255, 255, 0.18);
  stroke-width: 1;
}

.xlabel {
  fill: rgba(255, 255, 255, 0.45);
  font-size: 9px;
}

.empty {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0;
  font-size: 12px;
  opacity: 0.5;
}
</style>
