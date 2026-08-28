<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import {
  NDataTable,
  NButton,
  NInput,
  NSelect,
  NModal,
  NCard,
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
import PageHeader from '@/components/common/PageHeader.vue'
import AppIcon from '@/components/icon/AppIcon.vue'
import ProfileForm from './ProfileForm.vue'

const { t } = useI18n()
const message = useMessage()
const store = useProfileStore()
const environmentStore = useEnvironmentStore()

const search = ref('')
const deleting = ref<FrpsProfile | null>(null)
const deleteLoading = ref(false)
const formVisible = ref(false)
const editingProfile = ref<FrpsProfile | null>(null)

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
        h(NButton, { size: 'small', onClick: () => openEdit(row) }, { default: () => t('common.edit') }),
        h(NButton, { size: 'small', type: 'error', style: { marginLeft: '8px' }, onClick: () => (deleting.value = row) }, { default: () => t('common.delete') }),
      ]
    },
  },
]

function openCreate() {
  editingProfile.value = null
  formVisible.value = true
}

function openEdit(profile: FrpsProfile) {
  editingProfile.value = profile
  formVisible.value = true
}

function closeForm() {
  formVisible.value = false
  editingProfile.value = null
}

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
    <PageHeader :title="t('nav.profiles')">
      <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="profiles" :size="21" /></span></template>
      <template #actions>
        <NInput v-model:value="search" :placeholder="t('common.search')" clearable style="width: min(240px, 100%)"><template #prefix><AppIcon name="search" :size="16" /></template></NInput>
        <NButton type="primary" @click="openCreate">{{ t('profile.create') }}</NButton>
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
      :title="t('profile.delete')"
      :content="t('profile.deleteConfirm', { name: deleting?.name ?? '' })"
      :loading="deleteLoading"
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />

    <NModal v-model:show="formVisible" :mask-closable="false">
      <NCard
        :title="editingProfile ? t('profile.edit') : t('profile.create')"
        :bordered="false"
        closable
        role="dialog"
        aria-modal="true"
        style="width: min(720px, calc(100vw - 32px)); max-height: calc(100vh - 48px)"
        content-style="overflow-y: auto"
        @close="closeForm"
      >
        <ProfileForm
          v-if="formVisible"
          embedded
          :profile="editingProfile"
          @saved="closeForm"
          @cancel="closeForm"
        />
      </NCard>
    </NModal>
  </div>
</template>
