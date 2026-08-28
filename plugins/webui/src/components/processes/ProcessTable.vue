<script setup lang="ts">
import { computed, h } from 'vue'
import { NDataTable, NText, type DataTableColumns } from 'naive-ui'
import type { ProcessInfo } from '@/api/types'
import StatusBadge from '@/components/common/StatusBadge.vue'

const props = defineProps<{ processes: ProcessInfo[] }>()
const rows = computed(() => [...props.processes].sort((a, b) => Number(b.running) - Number(a.running) || a.profile_name.localeCompare(b.profile_name)))
const columns: DataTableColumns<ProcessInfo> = [
  { title: 'Profile', key: 'profile_name', sorter: 'default', minWidth: 150 },
  { title: 'PID', key: 'pid', width: 100, render: (row) => row.pid ?? '—' },
  { title: 'Status', key: 'running', width: 125, render: (row) => h(StatusBadge, { status: row.running ? 'running' : 'stopped', label: row.running ? 'Running' : 'Stopped' }) },
  { title: 'Restarts', key: 'restart_count', width: 110, sorter: (a, b) => a.restart_count - b.restart_count },
  { title: 'Config', key: 'config_path', minWidth: 220, ellipsis: { tooltip: true }, render: (row) => h(NText, { depth: 3, code: true }, { default: () => row.config_path }) },
]
</script>

<template>
  <NDataTable :columns="columns" :data="rows" :row-key="row => row.profile_id" :scroll-x="760" striped />
</template>
