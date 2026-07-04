<script setup lang="ts">
import { onMounted, ref, h } from 'vue'
import {
  NDataTable,
  NButton,
  NSpace,
  NSwitch,
  NSelect,
  NTag,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useBindingStore } from '@/stores/bindings'
import { useProfileStore } from '@/stores/profiles'
import { useProxyStore } from '@/stores/proxies'
import { resolveErrorMessage } from '@/api/errors'
import type { BindingRule } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'

const { t } = useI18n()
const message = useMessage()
const store = useBindingStore()
const profileStore = useProfileStore()
const proxyStore = useProxyStore()

const showCreate = ref(false)
const deleting = ref<BindingRule | null>(null)
const deleteLoading = ref(false)
const actionLoading = ref<number | null>(null) // binding id currently being started/stopped

const newBinding = ref<Partial<BindingRule>>({
  profile_id: 0,
  proxy_id: 0,
  enabled: true,
  running: false,
  priority: 100,
  group_name: null,
  group_key: null,
})

// ── Status helper ──

type BindingStatus = 'running' | 'standby' | 'disabled' | 'error'

function getStatus(row: BindingRule): BindingStatus {
  if (!row.enabled) return 'disabled'
  if (row.running) return 'running'
  return 'standby'
}

const statusColor: Record<BindingStatus, string> = {
  running: '#18a058',
  standby: '#f0a020',
  disabled: '#999',
  error: '#d03050',
}

const statusIcon: Record<BindingStatus, string> = {
  running: '●',
  standby: '◉',
  disabled: '○',
  error: '⚠',
}

// ── Columns ──

const columns: DataTableColumns<BindingRule> = [
  { title: 'ID', key: 'id', width: 60 },
  {
    title: 'Profile',
    key: 'profile_id',
    width: 120,
    render: (row) =>
      profileStore.profiles.find((p) => p.id === row.profile_id)?.name ?? `#${row.profile_id}`,
  },
  {
    title: 'Proxy',
    key: 'proxy_id',
    width: 100,
    render: (row) =>
      proxyStore.proxies.find((p) => p.id === row.proxy_id)?.name ?? `#${row.proxy_id}`,
  },
  { title: 'Priority', key: 'priority', width: 80 },
  { title: 'Group', key: 'group_name', width: 100, render: (row) => row.group_name ?? '-' },
  {
    title: t('common.enabled'),
    key: 'enabled',
    width: 90,
    render(row) {
      return h(NSwitch, {
        value: row.enabled,
        size: 'small',
        onUpdateValue: (val: boolean) => handleToggle(row, val),
      })
    },
  },
  {
    title: 'Status',
    key: 'status',
    width: 120,
    render(row) {
      const status = getStatus(row)
      return h(
        NTag,
        {
          color: { color: statusColor[status], textColor: '#fff' },
          size: 'small',
          round: true,
        },
        { default: () => `${statusIcon[status]} ${t(`binding.status.${status}`)}` },
      )
    },
  },
  {
    title: t('common.actions'),
    key: 'actions',
    width: 180,
    render(row) {
      const status = getStatus(row)
      const children: any[] = []

      // Start button (for standby bindings)
      if (status === 'standby') {
        children.push(
          h(
            NButton,
            {
              size: 'tiny',
              type: 'success',
              loading: actionLoading.value === row.id,
              disabled: actionLoading.value !== null,
              onClick: () => handleStart(row),
            },
            { default: () => t('binding.start') },
          ),
        )
      }

      // Stop button (for running bindings)
      if (status === 'running') {
        children.push(
          h(
            NButton,
            {
              size: 'tiny',
              type: 'warning',
              loading: actionLoading.value === row.id,
              disabled: actionLoading.value !== null,
              onClick: () => handleStop(row),
            },
            { default: () => t('binding.stop') },
          ),
        )
      }

      // Delete button
      children.push(
        h(
          NButton,
          {
            size: 'tiny',
            type: 'error',
            style: children.length > 0 ? { marginLeft: '6px' } : undefined,
            onClick: () => (deleting.value = row),
          },
          { default: () => t('common.delete') },
        ),
      )

      return h(NSpace, { size: 4 }, { default: () => children })
    },
  },
]

// ── Handlers ──

async function handleToggle(binding: BindingRule, enabled: boolean) {
  if (!binding.id) return
  try {
    await store.toggle(binding.id, enabled)
    // If disabling a running binding, the API auto-stops it.
    // Re-fetch to get the correct running state from the server.
    if (!enabled && binding.running) {
      await store.fetchAll()
    }
    message.success(enabled ? t('binding.toggleOn') : t('binding.toggleOff'))
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  }
}

async function handleStart(binding: BindingRule) {
  if (!binding.id) return
  actionLoading.value = binding.id
  try {
    await store.startBinding(binding.id)
    message.success(t('binding.startSuccess'))
    // Refresh to get accurate state from server
    await store.fetchAll()
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    actionLoading.value = null
  }
}

async function handleStop(binding: BindingRule) {
  if (!binding.id) return
  actionLoading.value = binding.id
  try {
    await store.stopBinding(binding.id)
    message.success(t('binding.stopSuccess'))
    // Refresh to get accurate state from server
    await store.fetchAll()
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    actionLoading.value = null
  }
}

async function handleCreate() {
  if (!newBinding.value.profile_id || !newBinding.value.proxy_id) {
    message.error('Please select both profile and proxy')
    return
  }
  try {
    await store.create(newBinding.value as BindingRule)
    message.success(t('binding.createSuccess'))
    showCreate.value = false
    newBinding.value = {
      profile_id: 0,
      proxy_id: 0,
      enabled: true,
      running: false,
      priority: 100,
      group_name: null,
      group_key: null,
    }
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  }
}

async function confirmDelete() {
  if (!deleting.value?.id) return
  deleteLoading.value = true
  try {
    await store.remove(deleting.value.id)
    message.success(t('binding.deleteSuccess'))
    deleting.value = null
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    deleteLoading.value = false
  }
}

onMounted(async () => {
  await Promise.all([store.fetchAll(), profileStore.fetchAll(), proxyStore.fetchAll()])
})
</script>

<template>
  <div>
    <ErrorAlert :error="store.error" @dismiss="store.error = null" />

    <NSpace justify="space-between" style="margin-bottom: 16px">
      <span></span>
      <NButton type="primary" @click="showCreate = !showCreate">
        {{ t('binding.create') }}
      </NButton>
    </NSpace>

    <!-- Create form -->
    <div
      v-if="showCreate"
      style="margin-bottom: 16px; padding: 12px; border: 1px solid var(--n-border-color); border-radius: 4px"
    >
      <NSpace align="center">
        <NSelect
          v-model:value="newBinding.profile_id"
          :options="profileStore.profiles.map((p) => ({ label: p.name, value: p.id! }))"
          placeholder="Profile"
          style="width: 180px"
        />
        <NSelect
          v-model:value="newBinding.proxy_id"
          :options="proxyStore.proxies.map((p) => ({ label: p.name, value: p.id! }))"
          placeholder="Proxy"
          style="width: 180px"
        />
        <NSwitch v-model:value="newBinding.enabled" />
        <NButton type="primary" size="small" @click="handleCreate">
          {{ t('common.save') }}
        </NButton>
        <NButton size="small" @click="showCreate = false">
          {{ t('common.cancel') }}
        </NButton>
      </NSpace>
    </div>

    <NDataTable
      :columns="columns"
      :data="store.bindings"
      :loading="store.loading"
      :bordered="false"
    />

    <ConfirmDialog
      :show="!!deleting"
      :title="t('binding.delete')"
      :content="t('binding.deleteConfirm')"
      :loading="deleteLoading"
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />
  </div>
</template>
