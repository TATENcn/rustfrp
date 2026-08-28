<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  NCard,
  NForm,
  NFormItem,
  NInput,
  NButton,
  NSpace,
  useMessage,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { healthCheck } from '@/api/system'
import AppIcon from '@/components/icon/AppIcon.vue'

const router = useRouter()
const { t } = useI18n()
const message = useMessage()

const token = ref('')
const loading = ref(false)

async function handleLogin() {
  if (!token.value.trim()) return

  loading.value = true
  try {
    localStorage.setItem('api_token', token.value.trim())

    // Verify the token works with a health check
    await healthCheck()

    message.success('Logged in')
    router.replace({ name: 'dashboard' })
  } catch {
    localStorage.removeItem('api_token')
    message.error(t('error.unauthorized'))
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="grid min-h-screen place-items-center bg-canvas p-6">
    <NCard class="w-full max-w-md shadow-panel" :bordered="false">
      <div class="mb-7 flex flex-col items-center text-center">
        <span class="mb-4 grid size-12 place-items-center rounded-2xl bg-primary text-white"><AppIcon name="proxies" :size="24" /></span>
        <h1 class="m-0 text-xl font-semibold text-foreground">{{ t('app.title') }}</h1>
        <p class="mt-2 mb-0 text-sm text-foreground-muted">Secure access to the RustFRP control plane</p>
      </div>
      <NForm @submit.prevent="handleLogin">
        <NFormItem :label="t('auth.tokenLabel')">
          <NInput
            v-model:value="token"
            type="password"
            :placeholder="t('auth.tokenLabel')"
            :disabled="loading"
          />
        </NFormItem>
        <NSpace vertical>
          <NButton block type="primary" :loading="loading" @click="handleLogin">
            {{ t('auth.login') }}
          </NButton>
        </NSpace>
      </NForm>
    </NCard>
  </div>
</template>
