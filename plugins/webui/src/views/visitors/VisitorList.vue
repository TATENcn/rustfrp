<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NInputNumber,
  NSelect,
  NSwitch,
  NModal,
  NCard,
  NForm,
  NFormItem,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useVisitorStore } from '@/stores/visitors'
import { useProfileStore } from '@/stores/profiles'
import { resolveErrorMessage } from '@/api/errors'
import type { LocalVisitor, VisitorType } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import AppIcon from '@/components/icon/AppIcon.vue'

const { t } = useI18n()
const message = useMessage()
const store = useVisitorStore()
const profileStore = useProfileStore()

const search = ref('')
const showCreate = ref(false)
const deleting = ref<LocalVisitor | null>(null)
const deleteLoading = ref(false)
const saving = ref(false)

const defaultVisitor = (): Partial<LocalVisitor> => ({
  name: '',
  visitor_type: 'stcp',
  server_name: '',
  server_user: null,
  bind_addr: null,
  bind_port: -1,
  secret_key: null,
  enabled: true,
  use_encryption: true,
  use_compression: true,
  profile_id: 0,
})
const newVisitor = ref<Partial<LocalVisitor>>(defaultVisitor())

const typeOptions: { label: string; value: VisitorType }[] = [
  { label: 'STCP', value: 'stcp' },
  { label: 'SUDP', value: 'sudp' },
  { label: 'XTCP', value: 'xtcp' },
]

const columns: DataTableColumns<LocalVisitor> = [
  { title: t('common.name'), key: 'name', sorter: true },
  { title: 'Type', key: 'visitor_type', width: 70 },
  { title: 'Server Name', key: 'server_name', width: 140 },
  { title: 'Bind', key: 'bind_addr', width: 140, render: (row) => row.bind_addr ? `${row.bind_addr}:${row.bind_port}` : `Port ${row.bind_port}` },
  { title: 'Profile', key: 'profile_id', width: 110, render: (row) => profileStore.profiles.find(p => p.id === row.profile_id)?.name ?? `#${row.profile_id}` },
  {
    title: t('common.enabled'),
    key: 'enabled',
    width: 80,
    render(row) {
      return row.enabled ? t('common.enabled') : t('common.disabled')
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

const filtered = computed(() => {
  if (!search.value) return store.visitors
  const q = search.value.toLowerCase()
  return store.visitors.filter(
    (v) =>
      v.name.toLowerCase().includes(q) ||
      v.visitor_type.toLowerCase().includes(q) ||
      v.server_name.toLowerCase().includes(q),
  )
})

const editing = ref<LocalVisitor | null>(null)

function openEdit(visitor: LocalVisitor) {
  editing.value = { ...visitor }
}

function closeCreate() {
  showCreate.value = false
  newVisitor.value = defaultVisitor()
}

async function handleCreate() {
  if (!newVisitor.value.name || !newVisitor.value.server_name) {
    message.error('Name and Server Name are required')
    return
  }
  saving.value = true
  try {
    await store.create(newVisitor.value as LocalVisitor)
    message.success(t('visitor.createSuccess'))
    closeCreate()
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    saving.value = false
  }
}

async function handleUpdate() {
  if (!editing.value?.id) return
  saving.value = true
  try {
    await store.update(editing.value.id, editing.value)
    message.success(t('visitor.updateSuccess'))
    editing.value = null
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    saving.value = false
  }
}

async function confirmDelete() {
  if (!deleting.value?.id) return
  deleteLoading.value = true
  try {
    await store.remove(deleting.value.id)
    message.success(t('visitor.deleteSuccess'))
    deleting.value = null
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    deleteLoading.value = false
  }
}

onMounted(async () => {
  await Promise.all([store.fetchAll(), profileStore.fetchAll()])
})
</script>

<template>
  <div>
    <PageHeader :title="t('nav.visitors')">
      <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="visitors" :size="21" /></span></template>
      <template #actions><NInput v-model:value="search" :placeholder="t('common.search')" clearable style="width: min(240px, 100%)"><template #prefix><AppIcon name="search" :size="16" /></template></NInput><NButton type="primary" @click="showCreate = true">{{ t('visitor.create') }}</NButton></template>
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
      :title="t('visitor.delete')"
      :content="t('visitor.deleteConfirm', { name: deleting?.name ?? '' })"
      :loading="deleteLoading"
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />

    <NModal v-model:show="showCreate" :mask-closable="false">
      <NCard
        :title="t('visitor.create')"
        :bordered="false"
        closable
        role="dialog"
        aria-modal="true"
        style="width: min(640px, calc(100vw - 32px)); max-height: calc(100vh - 48px)"
        content-style="overflow-y: auto"
        @close="closeCreate"
      >
        <NForm label-placement="top">
          <NFormItem label="Name" required><NInput v-model:value="newVisitor.name" /></NFormItem>
          <NFormItem label="Type"><NSelect v-model:value="newVisitor.visitor_type" :options="typeOptions" /></NFormItem>
          <NFormItem label="Server Name" required><NInput v-model:value="newVisitor.server_name" /></NFormItem>
          <NFormItem label="Server User"><NInput v-model:value="newVisitor.server_user" /></NFormItem>
          <NFormItem label="Bind Address"><NInput v-model:value="newVisitor.bind_addr" placeholder="127.0.0.1" /></NFormItem>
          <NFormItem label="Bind Port"><NInputNumber v-model:value="newVisitor.bind_port" :min="-1" :max="65535" /></NFormItem>
          <NFormItem label="Secret Key"><NInput v-model:value="newVisitor.secret_key" type="password" show-password-on="click" /></NFormItem>
          <NFormItem label="Profile">
            <NSelect v-model:value="newVisitor.profile_id" :options="profileStore.profiles.map(p => ({ label: p.name, value: p.id! }))" />
          </NFormItem>
          <NFormItem :label="t('common.enabled')"><NSwitch v-model:value="newVisitor.enabled" /></NFormItem>
          <NFormItem label="Encryption"><NSwitch v-model:value="newVisitor.use_encryption" /></NFormItem>
          <NFormItem label="Compression"><NSwitch v-model:value="newVisitor.use_compression" /></NFormItem>
          <NSpace justify="end">
            <NButton @click="closeCreate">{{ t('common.cancel') }}</NButton>
            <NButton type="primary" :loading="saving" @click="handleCreate">{{ t('common.save') }}</NButton>
          </NSpace>
        </NForm>
      </NCard>
    </NModal>

    <NModal :show="!!editing" :mask-closable="false" @update:show="value => { if (!value) editing = null }">
      <NCard
        v-if="editing"
        :title="t('visitor.edit')"
        :bordered="false"
        closable
        role="dialog"
        aria-modal="true"
        style="width: min(640px, calc(100vw - 32px)); max-height: calc(100vh - 48px)"
        content-style="overflow-y: auto"
        @close="editing = null"
      >
        <NForm label-placement="top">
          <NFormItem label="Name" required><NInput v-model:value="editing.name" /></NFormItem>
          <NFormItem label="Type"><NSelect v-model:value="editing.visitor_type" :options="typeOptions" /></NFormItem>
          <NFormItem label="Server Name" required><NInput v-model:value="editing.server_name" /></NFormItem>
          <NFormItem label="Server User"><NInput v-model:value="editing.server_user" /></NFormItem>
          <NFormItem label="Bind Address"><NInput v-model:value="editing.bind_addr" /></NFormItem>
          <NFormItem label="Bind Port"><NInputNumber v-model:value="editing.bind_port" :min="-1" :max="65535" /></NFormItem>
          <NFormItem label="Secret Key"><NInput v-model:value="editing.secret_key" type="password" show-password-on="click" /></NFormItem>
          <NFormItem label="Profile">
            <NSelect v-model:value="editing.profile_id" :options="profileStore.profiles.map(p => ({ label: p.name, value: p.id! }))" />
          </NFormItem>
          <NFormItem :label="t('common.enabled')"><NSwitch v-model:value="editing.enabled" /></NFormItem>
          <NFormItem label="Encryption"><NSwitch v-model:value="editing.use_encryption" /></NFormItem>
          <NFormItem label="Compression"><NSwitch v-model:value="editing.use_compression" /></NFormItem>
          <NSpace justify="end">
            <NButton @click="editing = null">{{ t('common.cancel') }}</NButton>
            <NButton type="primary" :loading="saving" @click="handleUpdate">{{ t('common.save') }}</NButton>
          </NSpace>
        </NForm>
      </NCard>
    </NModal>
  </div>
</template>
