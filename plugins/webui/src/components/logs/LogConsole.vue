<script setup lang="ts">
import { computed } from 'vue'
import { NEmpty, NVirtualList } from 'naive-ui'

const props = defineProps<{ content: string; search?: string; stream?: string }>()
interface LogLine { id: string; text: string; level: string; number: number }
const items = computed<LogLine[]>(() => {
  const query = props.search?.trim().toLocaleLowerCase()
  return props.content.split('\n').map((text, index) => ({
    id: `${index}-${text.slice(0, 24)}`,
    text,
    number: index + 1,
    level: /\berror|fatal|failed\b/i.test(text) ? 'error' : /\bwarn(?:ing)?\b/i.test(text) ? 'warn' : /\bdebug|trace\b/i.test(text) ? 'debug' : 'info',
  })).filter(item => item.text && (!query || item.text.toLocaleLowerCase().includes(query)))
})
</script>

<template>
  <div class="log-console min-h-[420px] overflow-hidden rounded-b-card bg-[#0b0f17] text-[#d6deeb]">
    <NVirtualList v-if="items.length" :items="items" :item-size="28" item-resizable class="h-[min(64vh,720px)]" item-key="id">
      <template #default="{ item }">
        <div class="group flex min-h-7 items-start font-mono text-xs leading-7 hover:bg-white/5">
          <span class="w-14 shrink-0 select-none pr-3 text-right text-slate-600">{{ item.number }}</span>
          <span class="mt-[11px] mr-3 size-1.5 shrink-0 rounded-full" :class="item.level === 'error' ? 'bg-red-400' : item.level === 'warn' ? 'bg-amber-400' : item.level === 'debug' ? 'bg-violet-400' : 'bg-sky-400'" />
          <span class="min-w-0 flex-1 whitespace-pre-wrap break-all pr-4" :class="item.level === 'error' ? 'text-red-300' : item.level === 'warn' ? 'text-amber-200' : 'text-slate-300'">{{ item.text }}</span>
        </div>
      </template>
    </NVirtualList>
    <NEmpty v-else class="py-32" description="No matching logs" />
  </div>
</template>
