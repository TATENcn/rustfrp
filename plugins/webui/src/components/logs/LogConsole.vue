<script setup lang="ts">
import { computed } from 'vue'
import { NEmpty, NVirtualList } from 'naive-ui'
import { formatLogLines } from './format'

const props = defineProps<{ content: string; search?: string; stream?: string }>()
const items = computed(() => {
  const query = props.search?.trim().toLocaleLowerCase()
  return formatLogLines(props.content)
    .filter(item => item.text && (!query || item.text.toLocaleLowerCase().includes(query)))
})
</script>

<template>
  <div class="log-console min-h-[420px] overflow-hidden rounded-b-card bg-[#0b0f17] text-[#d6deeb]">
    <NVirtualList v-if="items.length" :items="items" :item-size="28" item-resizable class="h-[min(64vh,720px)]" item-key="id">
      <template #default="{ item }">
        <div class="group flex min-h-7 items-start font-mono text-xs leading-7 hover:bg-white/5">
          <span class="w-14 shrink-0 select-none pr-3 text-right text-slate-600">{{ item.number }}</span>
          <span class="mt-[11px] mr-3 size-1.5 shrink-0 rounded-full" :class="item.level === 'error' ? 'bg-red-400' : item.level === 'warn' ? 'bg-amber-400' : item.level === 'debug' ? 'bg-violet-400' : 'bg-sky-400'" />
          <span
            class="min-w-0 flex-1 whitespace-pre-wrap break-all pr-4"
            :class="item.level === 'error' ? 'text-red-300' : item.level === 'warn' ? 'text-amber-200' : item.level === 'debug' ? 'text-violet-300' : 'text-slate-300'"
            v-html="item.html"
          />
        </div>
      </template>
    </NVirtualList>
    <NEmpty v-else class="py-32" description="No matching logs" />
  </div>
</template>

<style scoped>
.log-console :deep(.ansi-black-fg) { color: #64748b; }
.log-console :deep(.ansi-red-fg), .log-console :deep(.ansi-bright-red-fg) { color: #f87171; }
.log-console :deep(.ansi-green-fg), .log-console :deep(.ansi-bright-green-fg) { color: #4ade80; }
.log-console :deep(.ansi-yellow-fg), .log-console :deep(.ansi-bright-yellow-fg) { color: #facc15; }
.log-console :deep(.ansi-blue-fg), .log-console :deep(.ansi-bright-blue-fg) { color: #60a5fa; }
.log-console :deep(.ansi-magenta-fg), .log-console :deep(.ansi-bright-magenta-fg) { color: #c084fc; }
.log-console :deep(.ansi-cyan-fg), .log-console :deep(.ansi-bright-cyan-fg) { color: #22d3ee; }
.log-console :deep(.ansi-white-fg), .log-console :deep(.ansi-bright-white-fg) { color: #f1f5f9; }
.log-console :deep(.ansi-black-bg) { background-color: #0f172a; }
.log-console :deep(.ansi-red-bg), .log-console :deep(.ansi-bright-red-bg) { background-color: #7f1d1d; }
.log-console :deep(.ansi-green-bg), .log-console :deep(.ansi-bright-green-bg) { background-color: #14532d; }
.log-console :deep(.ansi-yellow-bg), .log-console :deep(.ansi-bright-yellow-bg) { background-color: #713f12; }
.log-console :deep(.ansi-blue-bg), .log-console :deep(.ansi-bright-blue-bg) { background-color: #1e3a8a; }
.log-console :deep(.ansi-magenta-bg), .log-console :deep(.ansi-bright-magenta-bg) { background-color: #581c87; }
.log-console :deep(.ansi-cyan-bg), .log-console :deep(.ansi-bright-cyan-bg) { background-color: #164e63; }
.log-console :deep(.ansi-white-bg), .log-console :deep(.ansi-bright-white-bg) { background-color: #475569; }
</style>
