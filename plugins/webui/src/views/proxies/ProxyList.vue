<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import { useRouter } from 'vue-router'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NModal,
  NCard,
  NForm,
  NFormItem,
  NSelect,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useProxyStore } from '@/stores/proxies'
import { useBindingStore } from '@/stores/bindings'
import { useProfileStore } from '@/stores/profiles'
import { resolveErrorMessage } from '@/api/errors'
import type { LocalProxy } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import AppIcon from '@/components/icon/AppIcon.vue'

const router = useRouter()
const { t } = useI18n()
const message = useMessage()
const store = useProxyStore()
const bindingStore = useBindingStore()
const profileStore = useProfileStore()

const search = ref('')
const deleting = ref<LocalProxy | null>(null)
const deleteLoading = ref(false)
const batchVisible = ref(false)
const batchLoading = ref(false)
const batchPorts = ref('')
const batchType = ref<'tcp' | 'udp'>('tcp')
const batchLocalIp = ref('127.0.0.1')

function mappedAddresses(proxy: LocalProxy): string[] {
  if (!proxy.id) return []
  const profiles = bindingStore.bindings
    .filter((binding) => binding.proxy_id === proxy.id && binding.enabled)
    .map((binding) => profileStore.profiles.find((profile) => profile.id === binding.profile_id))
    .filter((profile) => profile !== undefined)

  const addresses: string[] = []
  for (const profile of profiles) {
    if ((proxy.proxy_type === 'tcp' || proxy.proxy_type === 'udp') && proxy.remote_port) {
      addresses.push(`${profile.server_addr}:${proxy.remote_port}`)
    } else if (proxy.proxy_type === 'http' || proxy.proxy_type === 'https') {
      const scheme = proxy.proxy_type
      for (const domain of proxy.custom_domains ?? []) addresses.push(`${scheme}://${domain}`)
    }
  }
  return [...new Set(addresses)]
}

async function writeClipboard(value: string) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(value)
    return
  }
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  const copied = document.execCommand('copy')
  textarea.remove()
  if (!copied) throw new Error('clipboard unavailable')
}

async function copyMappedAddress(proxy: LocalProxy) {
  const addresses = mappedAddresses(proxy)
  if (!addresses.length) {
    message.warning(t('proxy.noMappedAddress'))
    return
  }
  try {
    await writeClipboard(addresses.join('\n'))
    message.success(t('proxy.copySuccess', { count: addresses.length }))
  } catch {
    message.error(t('proxy.copyFailed'))
  }
}

function parsePortSpec(spec: string): number[] {
  const ports = new Set<number>()
  for (const token of spec.split(/[\s,，]+/).filter(Boolean)) {
    const range = token.match(/^(\d+)-(\d+)$/)
    if (range) {
      const start = Number(range[1])
      const end = Number(range[2])
      if (start > end || start < 1 || end > 65535) throw new Error('invalid')
      for (let port = start; port <= end; port += 1) {
        ports.add(port)
        if (ports.size > 100) throw new Error('too_many')
      }
      continue
    }
    if (!/^\d+$/.test(token)) throw new Error('invalid')
    const port = Number(token)
    if (port < 1 || port > 65535) throw new Error('invalid')
    ports.add(port)
    if (ports.size > 100) throw new Error('too_many')
  }
  return [...ports].sort((a, b) => a - b)
}

async function createBatch() {
  let ports: number[]
  try {
    ports = parsePortSpec(batchPorts.value)
  } catch (error) {
    message.error(t(error instanceof Error && error.message === 'too_many' ? 'proxy.batchTooMany' : 'proxy.batchInvalid'))
    return
  }
  if (!ports.length || !batchLocalIp.value.trim()) {
    message.error(t('proxy.batchInvalid'))
    return
  }
  const conflicts = ports.filter((port) =>
    store.proxies.some((proxy) => proxy.name === `${batchType.value}-${port}`),
  )
  if (conflicts.length) {
    message.error(t('proxy.batchConflict', { ports: conflicts.join(', ') }))
    return
  }

  batchLoading.value = true
  let created = 0
  try {
    for (const port of ports) {
      await store.create({
        name: `${batchType.value}-${port}`,
        proxy_type: batchType.value,
        local_ip: batchLocalIp.value.trim(),
        local_port: port,
        remote_port: port,
        use_encryption: true,
        use_compression: true,
        health_check_timeout_s: 3,
        health_check_max_failed: 3,
        health_check_interval_s: 10,
        created_at: '',
        updated_at: '',
      })
      created += 1
    }
    message.success(t('proxy.batchSuccess', { count: created }))
    batchVisible.value = false
    batchPorts.value = ''
  } catch (error: any) {
    message.error(`${t('proxy.batchPartial', { created })}: ${t(resolveErrorMessage(error?.code))}`)
  } finally {
    batchLoading.value = false
  }
}

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
    width: 250,
    render(row) {
      return [
        h(NButton, { size: 'small', disabled: mappedAddresses(row).length === 0, onClick: () => copyMappedAddress(row) }, { default: () => t('proxy.copyAddress') }),
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

onMounted(() => Promise.all([store.fetchAll(), bindingStore.fetchAll(), profileStore.fetchAll()]))
</script>

<template>
  <div>
    <PageHeader :title="t('nav.proxies')">
      <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="proxies" :size="21" /></span></template>
      <template #actions>
        <NInput v-model:value="search" :placeholder="t('common.search')" clearable class="w-60"><template #prefix><AppIcon name="search" :size="16" /></template></NInput>
        <NButton @click="batchVisible = true">{{ t('proxy.batchCreate') }}</NButton>
        <NButton type="primary" @click="router.push({ name: 'proxy-new' })">{{ t('proxy.create') }}</NButton>
      </template>
    </PageHeader>
    <ErrorAlert :error="store.error" @dismiss="store.error = null" />

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

    <NModal v-model:show="batchVisible">
      <NCard :title="t('proxy.batchCreate')" style="width: min(560px, 92vw)" closable @close="batchVisible = false">
        <NForm label-placement="top">
          <NFormItem :label="t('proxy.type')">
            <NSelect v-model:value="batchType" :options="[{ label: 'TCP', value: 'tcp' }, { label: 'UDP', value: 'udp' }]" />
          </NFormItem>
          <NFormItem :label="t('proxy.localIp')">
            <NInput v-model:value="batchLocalIp" />
          </NFormItem>
          <NFormItem :label="t('proxy.batchPorts')" :feedback="t('proxy.batchPortsHint')">
            <NInput v-model:value="batchPorts" type="textarea" :placeholder="t('proxy.batchPortsPlaceholder')" :autosize="{ minRows: 3, maxRows: 6 }" />
          </NFormItem>
          <NSpace justify="end">
            <NButton @click="batchVisible = false">{{ t('common.cancel') }}</NButton>
            <NButton type="primary" :loading="batchLoading" @click="createBatch">{{ t('common.create') }}</NButton>
          </NSpace>
        </NForm>
      </NCard>
    </NModal>
  </div>
</template>
