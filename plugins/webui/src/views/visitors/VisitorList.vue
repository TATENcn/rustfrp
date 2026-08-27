<script setup lang="ts">
import { onMounted, ref, computed, h } from 'vue'
import {
  NDataTable,
  NButton,
  NSpace,
  NInput,
  NSelect,
  NSwitch,
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

const newVisitor = ref<Partial<LocalVisitor>>({
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

async function handleCreate() {
  if (!newVisitor.value.name || !newVisitor.value.server_name) {
    message.error('Name and Server Name are required')
    return
  }
  saving.value = true
  try {
    await store.create(newVisitor.value as LocalVisitor)
    message.success(t('visitor.createSuccess'))
    showCreate.value = false
    newVisitor.value = { name: '', visitor_type: 'stcp', server_name: '', server_user: null, bind_addr: null, bind_port: -1, secret_key: null, enabled: true, use_encryption: true, use_compression: true, profile_id: 0 }
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
      <template #actions><NInput v-model:value="search" :placeholder="t('common.search')" clearable class="w-60"><template #prefix><AppIcon name="search" :size="16" /></template></NInput><NButton type="primary" @click="showCreate = !showCreate">{{ t('visitor.create') }}</NButton></template>
    </PageHeader>
    <ErrorAlert :error="store.error" @dismiss="store.error = null" />

    <!-- Create form -->
    <div v-if="showCreate" class="mb-4 rounded-card border border-border bg-surface p-4 shadow-card">
      <NSpace vertical class="w-full">
        <NSpace>
          <NInput v-model:value="newVisitor.name" placeholder="Name" style="width: 150px" />
          <NSelect v-model:value="newVisitor.visitor_type" :options="typeOptions" style="width: 100px" />
          <NInput v-model:value="newVisitor.server_name" placeholder="Server Name" style="width: 150px" />
          <NInput v-model:value="newVisitor.bind_addr" placeholder="Bind Addr (opt)" style="width: 140px" />
          <NInput v-model:value="newVisitor.secret_key" placeholder="Secret Key (opt)" style="width: 140px" />
          <NSelect
            v-model:value="newVisitor.profile_id"
            :options="profileStore.profiles.map(p => ({ label: p.name, value: p.id! }))"
            placeholder="Profile"
            style="width: 160px"
          />
        </NSpace>
        <NSpace>
          <NButton type="primary" size="small" :loading="saving" @click="handleCreate">{{ t('common.save') }}</NButton>
          <NButton size="small" @click="showCreate = false">{{ t('common.cancel') }}</NButton>
        </NSpace>
      </NSpace>
    </div>

    <!-- Edit modal -->
    <div v-if="editing" class="mb-4 rounded-card border border-info bg-surface p-4 shadow-card">
      <NSpace vertical class="w-full">
        <div class="font-semibold">{{ t('visitor.edit') }}: {{ editing.name }}</div>
        <NSpace>
          <NInput v-model:value="editing.name" placeholder="Name" style="width: 150px" />
          <NInput v-model:value="editing.server_name" placeholder="Server Name" style="width: 150px" />
          <NInput v-model:value="editing.bind_addr" placeholder="Bind Addr" style="width: 140px" />
          <NInput v-model:value="editing.secret_key" placeholder="Secret Key" style="width: 140px" />
          <NSelect
            v-model:value="editing.profile_id"
            :options="profileStore.profiles.map(p => ({ label: p.name, value: p.id! }))"
            style="width: 160px"
          />
          <NSwitch v-model:value="editing.enabled" />
        </NSpace>
        <NSpace>
          <NButton type="primary" size="small" :loading="saving" @click="handleUpdate">{{ t('common.save') }}</NButton>
          <NButton size="small" @click="editing = null">{{ t('common.cancel') }}</NButton>
        </NSpace>
      </NSpace>
    </div>

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
  </div>
</template>
