<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NCard, NCheckbox, NCheckboxGroup, NEmpty, NInput, NModal, NSkeleton, NTag, NTooltip, useMessage } from 'naive-ui'
import { useI18n } from '@/i18n'
import { useBindingStore } from '@/stores/bindings'
import { useProfileStore } from '@/stores/profiles'
import { useProxyStore } from '@/stores/proxies'
import { extractApiError } from '@/api/errors'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import AppIcon from '@/components/icon/AppIcon.vue'
import type { FrpsProfile } from '@/api/types'

const { t } = useI18n()
const message = useMessage()
const bindingStore = useBindingStore()
const profileStore = useProfileStore()
const proxyStore = useProxyStore()
const saving = ref<number | null>(null)
const controlling = ref<number | null>(null)
const refreshing = ref<number | null>(null)
const activeProfile = ref<FrpsProfile | null>(null)
const draftSelection = ref<number[]>([])
const proxySearch = ref('')
const loading = computed(() => bindingStore.loading || profileStore.loading || proxyStore.loading)
const filteredProxies = computed(() => {
  const query = proxySearch.value.trim().toLocaleLowerCase()
  if (!query) return proxyStore.proxies
  return proxyStore.proxies.filter(proxy => [proxy.name, proxy.proxy_type, proxy.local_ip, String(proxy.local_port)].some(value => value.toLocaleLowerCase().includes(query)))
})

function assignedProxyIds(profileId: number) {
  return bindingStore.bindings
    .filter(binding => binding.profile_id === profileId && binding.enabled)
    .map(binding => binding.proxy_id)
}

function assignedProxies(profileId: number) {
  const ids = new Set(assignedProxyIds(profileId))
  return proxyStore.proxies.filter(proxy => proxy.id && ids.has(proxy.id))
}

function openAssignment(profile: FrpsProfile) {
  activeProfile.value = profile
  draftSelection.value = assignedProxyIds(profile.id!)
  proxySearch.value = ''
}

function closeAssignment() {
  activeProfile.value = null
  draftSelection.value = []
  proxySearch.value = ''
}

async function saveAssignment() {
  const profileId = activeProfile.value?.id
  if (!profileId) return
  saving.value = profileId
  try {
    const wasRunning = profileStore.runtimes[profileId]?.running ?? false
    await profileStore.replaceProxies(profileId, draftSelection.value)
    await Promise.all([bindingStore.fetchAll(), profileStore.fetchRuntime(profileId)])
    message.success(wasRunning && draftSelection.value.length ? t('binding.assignmentSavedReloaded') : t('binding.assignmentSaved'))
    closeAssignment()
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    saving.value = null
  }
}

async function start(profileId: number) {
  controlling.value = profileId
  try {
    await profileStore.start(profileId)
    message.success(t('binding.profileStarted'))
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    controlling.value = null
  }
}

async function stop(profileId: number) {
  controlling.value = profileId
  try {
    await profileStore.stop(profileId)
    message.success(t('binding.profileStopped'))
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    controlling.value = null
  }
}

async function reload(profileId: number) {
  controlling.value = profileId
  try {
    await profileStore.start(profileId)
    message.success(t('binding.profileReloaded'))
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    controlling.value = null
  }
}

async function refreshRuntime(profileId: number) {
  refreshing.value = profileId
  try { await profileStore.fetchRuntime(profileId) }
  catch (error) { message.error(extractApiError(error).message) }
  finally { refreshing.value = null }
}

function statusFor(profileId: number) {
  const runtime = profileStore.runtimes[profileId]
  if (runtime?.running) return { status: 'running' as const, label: t('binding.profileRunning') }
  if (runtime?.desired_running) return { status: 'offline' as const, label: t('binding.profileOffline') }
  if (!assignedProxyIds(profileId).length) return { status: 'idle' as const, label: t('binding.profileUnconfigured') }
  return { status: 'stopped' as const, label: t('binding.profileStoppedState') }
}

onMounted(async () => {
  await Promise.all([bindingStore.fetchAll(), profileStore.fetchAll(), proxyStore.fetchAll()])
  await Promise.all(profileStore.profiles.filter((profile) => profile.id).map((profile) => profileStore.fetchRuntime(profile.id!)))
})
</script>

<template>
  <div>
    <PageHeader :title="t('binding.assignments')" :description="t('binding.assignmentsDescription')">
      <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="bindings" :size="21" /></span></template>
    </PageHeader>
    <ErrorAlert :error="bindingStore.error || profileStore.error || proxyStore.error" />

    <div v-if="loading && !profileStore.profiles.length" class="grid gap-4 lg:grid-cols-2">
      <NCard v-for="index in 4" :key="index"><NSkeleton text :repeat="4" /></NCard>
    </div>
    <NEmpty v-else-if="!profileStore.profiles.length" :description="t('binding.noProfiles')" />
    <div v-else class="grid gap-4 lg:grid-cols-2">
      <NCard v-for="profile in profileStore.profiles" :key="profile.id" class="shadow-card">
        <template #header>
          <div class="flex items-center gap-2">
            <span>{{ profile.name }}</span>
            <StatusBadge
              :status="statusFor(profile.id!).status"
              :label="statusFor(profile.id!).label"
            />
          </div>
        </template>
        <template #header-extra>
          <div class="flex items-center gap-1">
            <NTag size="small" :bordered="false">{{ assignedProxyIds(profile.id!).length }} / {{ proxyStore.proxies.length }}</NTag>
            <NTooltip>
              <template #trigger><NButton quaternary circle size="small" :aria-label="t('binding.configure')" @click="openAssignment(profile)"><template #icon><AppIcon name="settings" :size="16" /></template></NButton></template>
              {{ t('binding.configure') }}
            </NTooltip>
          </div>
        </template>

        <div class="grid grid-cols-2 gap-3 text-sm">
          <div><div class="text-xs text-foreground-muted">{{ t('binding.server') }}</div><div class="mt-1 font-medium">{{ profile.server_addr }}:{{ profile.server_port }}</div></div>
          <div><div class="text-xs text-foreground-muted">{{ t('binding.transport') }}</div><div class="mt-1 font-medium uppercase">{{ profile.transport_protocol }}</div></div>
        </div>
        <div class="mt-4 border-t border-border pt-4">
          <div class="mb-2 text-xs text-foreground-muted">{{ t('binding.assignedProxies') }}</div>
          <div v-if="assignedProxies(profile.id!).length" class="flex flex-wrap gap-2">
            <NTag v-for="proxy in assignedProxies(profile.id!)" :key="proxy.id" size="small" :bordered="false">{{ proxy.name }} · {{ proxy.proxy_type.toUpperCase() }}</NTag>
          </div>
          <NEmpty v-else size="small" :description="t('binding.noAssignedProxies')" />
        </div>

        <template #footer>
          <div class="flex items-center justify-between gap-2">
            <NButton quaternary :loading="refreshing === profile.id" @click="refreshRuntime(profile.id!)"><template #icon><AppIcon name="refresh" /></template>{{ t('common.refresh') }}</NButton>
            <div class="flex gap-2">
              <NButton v-if="profileStore.runtimes[profile.id!]?.running" :loading="controlling === profile.id" :disabled="controlling !== null" @click="reload(profile.id!)"><template #icon><AppIcon name="reload" /></template>{{ t('app.reload') }}</NButton>
              <NButton v-if="profileStore.runtimes[profile.id!]?.running || profileStore.runtimes[profile.id!]?.desired_running" type="warning" :loading="controlling === profile.id" :disabled="controlling !== null" @click="stop(profile.id!)">{{ t('binding.stopProfile') }}</NButton>
              <NButton v-else type="primary" :loading="controlling === profile.id" :disabled="controlling !== null || !assignedProxyIds(profile.id!).length" @click="start(profile.id!)">{{ t('binding.startProfile') }}</NButton>
            </div>
          </div>
        </template>
      </NCard>
    </div>

    <NModal :show="!!activeProfile" :mask-closable="false" @update:show="value => { if (!value) closeAssignment() }">
      <NCard
        :title="t('binding.configureTitle', { name: activeProfile?.name ?? '' })"
        :bordered="false"
        closable
        role="dialog"
        aria-modal="true"
        style="width: min(680px, calc(100vw - 32px)); max-height: calc(100vh - 48px)"
        content-style="overflow-y: auto"
        @close="closeAssignment"
      >
        <NInput v-model:value="proxySearch" clearable :placeholder="t('binding.searchProxies')" class="mb-4"><template #prefix><AppIcon name="search" :size="16" /></template></NInput>
        <NCheckboxGroup v-model:value="draftSelection">
          <div v-if="filteredProxies.length" class="grid gap-2 sm:grid-cols-2">
            <NCheckbox v-for="proxy in filteredProxies" :key="proxy.id" :value="proxy.id!" class="items-start rounded-lg border border-border p-3 transition-colors hover:bg-surface-subtle">
              <span class="min-w-0"><span class="block truncate text-sm font-medium">{{ proxy.name }}</span><span class="mt-1 block text-xs text-foreground-muted">{{ proxy.proxy_type.toUpperCase() }} · {{ proxy.local_ip }}:{{ proxy.local_port }}</span></span>
            </NCheckbox>
          </div>
          <NEmpty v-else :description="t('binding.noMatchingProxies')" />
        </NCheckboxGroup>
        <template #footer>
          <div class="flex items-center justify-between gap-3">
            <span class="text-xs text-foreground-muted">{{ t('binding.selectedCount', { count: draftSelection.length }) }}</span>
            <div class="flex gap-2"><NButton @click="closeAssignment">{{ t('common.cancel') }}</NButton><NButton type="primary" :loading="saving === activeProfile?.id" @click="saveAssignment">{{ t('common.save') }}</NButton></div>
          </div>
        </template>
      </NCard>
    </NModal>
  </div>
</template>
