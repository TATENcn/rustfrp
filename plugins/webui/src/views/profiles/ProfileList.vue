<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import { useRouter } from 'vue-router'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NSelect,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useProfileStore } from '@/stores/profiles'
import { useEnvironmentStore } from '@/stores/environments'
import { resolveErrorMessage } from '@/api/errors'
import type { FrpsProfile } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'

const router = useRouter()
const { t } = useI18n()
const message = useMessage()
const store = useProfileStore()
const environmentStore = useEnvironmentStore()

const search = ref('')
const deleting = ref<FrpsProfile | null>(null)
const deleteLoading = ref(false)

const columns: DataTableColumns<FrpsProfile> = [
  { title: t('common.name'), key: 'name', sorter: true },
  { title: 'Server Address', key: 'server_addr', width: 180 },
  { title: 'Port', key: 'server_port', width: 80 },
  { title: 'Transport', key: 'transport_protocol', width: 100 },
  { title: 'TLS', key: 'tls_enable', width: 70, render: (row) => (row.tls_enable ? 'Yes' : 'No') },
  {
    title: 'Environment',
    key: 'environment',
    width: 160,
    render(row) {
      const current = environmentStore.environments.find((item) => item.profile_ids.includes(row.id ?? -1))?.id
      return h(NSelect, {
        value: current,
        size: 'small',
        options: environmentStore.environments.map((item) => ({ label: item.name, value: item.id! })),
        onUpdateValue: (environmentId: number) => row.id && environmentStore.assignProfile(row.id, environmentId),
      })
    },
  },
  {
    title: t('common.actions'),
    key: 'actions',
    width: 160,
    render(row) {
      return [
        h(NButton, { size: 'small', onClick: () => router.push({ name: 'profile-edit', params: { id: row.id } }) }, { default: () => t('common.edit') }),
        h(NButton, { size: 'small', type: 'error', style: { marginLeft: '8px' }, onClick: () => (deleting.value = row) }, { default: () => t('common.delete') }),
      ]
    },
  },
]

const filtered = computed(() => {
  const scoped = environmentStore.active
    ? store.profiles.filter((profile) => environmentStore.active!.profile_ids.includes(profile.id ?? -1))
    : store.profiles
  if (!search.value) return scoped
  const q = search.value.toLowerCase()
  return scoped.filter(
    (p) =>
      p.name.toLowerCase().includes(q) ||
      p.server_addr.toLowerCase().includes(q),
  )
})

async function confirmDelete() {
  if (!deleting.value?.id) return
  deleteLoading.value = true
  try {
    await store.remove(deleting.value.id)
    message.success(t('profile.deleteSuccess'))
    deleting.value = null
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    deleteLoading.value = false
  }
}

onMounted(() => Promise.all([store.fetchAll(), environmentStore.fetchAll()]))
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
      <NButton type="primary" @click="router.push({ name: 'profile-new' })">
        {{ t('profile.create') }}
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
      :title="t('profile.delete')"
      :content="t('profile.deleteConfirm', { name: deleting?.name ?? '' })"
      :loading="deleteLoading"
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />
  </div>
</template>
