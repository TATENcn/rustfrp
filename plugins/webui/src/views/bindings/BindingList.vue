<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NCard, NCheckbox, NCheckboxGroup, NEmpty, NSkeleton, NSpace, NTag, useMessage } from 'naive-ui'
import { useI18n } from '@/i18n'
import { useBindingStore } from '@/stores/bindings'
import { useProfileStore } from '@/stores/profiles'
import { useProxyStore } from '@/stores/proxies'
import { extractApiError } from '@/api/errors'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import AppIcon from '@/components/icon/AppIcon.vue'

const { t } = useI18n()
const message = useMessage()
const bindingStore = useBindingStore()
const profileStore = useProfileStore()
const proxyStore = useProxyStore()
const selections = ref<Record<number, number[]>>({})
const saving = ref<number | null>(null)
const controlling = ref<number | null>(null)
const loading = computed(() => bindingStore.loading || profileStore.loading || proxyStore.loading)

function hydrateSelections() {
  for (const profile of profileStore.profiles) {
    if (!profile.id) continue
    selections.value[profile.id] = bindingStore.bindings
      .filter((binding) => binding.profile_id === profile.id && binding.enabled)
      .map((binding) => binding.proxy_id)
  }
}

async function save(profileId: number) {
  saving.value = profileId
  try {
    await profileStore.replaceProxies(profileId, selections.value[profileId] ?? [])
    await Promise.all([bindingStore.fetchAll(), profileStore.fetchRuntime(profileId)])
    hydrateSelections()
    message.success(t('binding.assignmentSaved'))
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    saving.value = null
  }
}

async function control(profileId: number, running: boolean) {
  controlling.value = profileId
  try {
    if (running) await profileStore.stop(profileId)
    else await profileStore.start(profileId)
    message.success(running ? t('binding.profileStopped') : t('binding.profileStarted'))
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    controlling.value = null
  }
}

onMounted(async () => {
  await Promise.all([bindingStore.fetchAll(), profileStore.fetchAll(), proxyStore.fetchAll()])
  hydrateSelections()
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
              :status="profileStore.runtimes[profile.id!]?.running ? 'running' : 'stopped'"
              :label="profileStore.runtimes[profile.id!]?.running ? t('binding.profileRunning') : t('binding.profileStoppedState')"
            />
          </div>
        </template>
        <template #header-extra>
          <NTag size="small" :bordered="false">{{ (selections[profile.id!] ?? []).length }} / {{ proxyStore.proxies.length }}</NTag>
        </template>

        <NCheckboxGroup v-model:value="selections[profile.id!]">
          <NSpace vertical>
            <NCheckbox v-for="proxy in proxyStore.proxies" :key="proxy.id" :value="proxy.id!">
              {{ proxy.name }} · {{ proxy.proxy_type.toUpperCase() }} · {{ proxy.local_ip }}:{{ proxy.local_port }}
            </NCheckbox>
          </NSpace>
        </NCheckboxGroup>
        <NEmpty v-if="!proxyStore.proxies.length" size="small" :description="t('binding.noProxies')" />

        <template #footer>
          <div class="flex justify-end gap-2">
            <NButton :loading="saving === profile.id" :disabled="controlling !== null" @click="save(profile.id!)">{{ t('common.save') }}</NButton>
            <NButton
              :type="profileStore.runtimes[profile.id!]?.running ? 'warning' : 'primary'"
              :loading="controlling === profile.id"
              :disabled="saving !== null"
              @click="control(profile.id!, profileStore.runtimes[profile.id!]?.running ?? false)"
            >
              {{ profileStore.runtimes[profile.id!]?.running ? t('binding.stopProfile') : t('binding.startProfile') }}
            </NButton>
          </div>
        </template>
      </NCard>
    </div>
  </div>
</template>
