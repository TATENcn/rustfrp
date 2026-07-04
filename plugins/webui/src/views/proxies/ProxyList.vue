<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import { useRouter } from 'vue-router'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useProxyStore } from '@/stores/proxies'
import { resolveErrorMessage } from '@/api/errors'
import type { LocalProxy } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'

const router = useRouter()
const { t } = useI18n()
const message = useMessage()
const store = useProxyStore()

const search = ref('')
const deleting = ref<LocalProxy | null>(null)
const deleteLoading = ref(false)

const columns: DataTableColumns<LocalProxy> = [
  { title: t('common.name'), key: 'name', sorter: true },
  { title: t('proxy.type'), key: 'proxy_type', width: 80 },
  { title: t('proxy.localIp'), key: 'local_ip', width: 130 },
  { title: t('proxy.localPort'), key: 'local_port', width: 80 },
  { title: t('proxy.remotePort'), key: 'remote_port', width: 80, render: (row) => row.remote_port ?? '-' },
  { title: t('proxy.encryption'), key: 'use_encryption', width: 90, render: (row) => (row.use_encryption ? 'Yes' : 'No') },
  { title: t('proxy.compression'), key: 'use_compression', width: 90, render: (row) => (row.use_compression ? 'Yes' : 'No') },
  {
    title: t('common.actions'),
    key: 'actions',
    width: 160,
    render(row) {
      return [
        h(NButton, { size: 'small', onClick: () => router.push({ name: 'proxy-edit', params: { id: row.id } }) }, { default: () => t('common.edit') }),
        h(NButton, { size: 'small', type: 'error', style: { marginLeft: '8px' }, onClick: () => (deleting.value = row) }, { default: () => t('common.delete') }),
      ]
    },
  },
]

const filtered = computed(() => {
  if (!search.value) return store.proxies
  const q = search.value.toLowerCase()
  return store.proxies.filter(
    (p) =>
      p.name.toLowerCase().includes(q) ||
      p.proxy_type.toLowerCase().includes(q) ||
      p.local_ip.toLowerCase().includes(q),
  )
})

async function confirmDelete() {
  if (!deleting.value?.id) return
  deleteLoading.value = true
  try {
    await store.remove(deleting.value.id)
    message.success(t('proxy.deleteSuccess'))
    deleting.value = null
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    deleteLoading.value = false
  }
}

onMounted(() => store.fetchAll())
</script>

<template>
  <div>
    <ErrorAlert :error="store.error" @dismiss="store.error = null" />

    <NSpace justify="space-between" style="margin-bottom: 16px">
      <NInput
        v-model:value="search"
        :placeholder="t('common.search')"
        clearable
        style="width: 240px"
      />
      <NButton type="primary" @click="router.push({ name: 'proxy-new' })">
        {{ t('proxy.create') }}
      </NButton>
    </NSpace>

    <NDataTable
      :columns="columns"
      :data="filtered"
      :loading="store.loading"
      :bordered="false"
    />

    <ConfirmDialog
      :show="!!deleting"
      :title="t('proxy.delete')"
      :content="t('proxy.deleteConfirm', { name: deleting?.name ?? '' })"
      :loading="deleteLoading"
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />
  </div>
</template>
