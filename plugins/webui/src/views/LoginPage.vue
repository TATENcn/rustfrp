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
  <div style="display: flex; justify-content: center; align-items: center; height: 100vh">
    <NCard style="width: 400px" :title="t('app.title')">
      <NForm @submit.prevent="handleLogin">
        <NFormItem :label="t('auth.tokenLabel')">
          <NInput
            v-model:value="token"
            type="password"
            :placeholder="t('auth.tokenLabel')"
            :disabled="loading"
          />
        </NFormItem>
        <NSpace justify="end">
          <NButton type="primary" :loading="loading" @click="handleLogin">
            {{ t('auth.login') }}
          </NButton>
        </NSpace>
      </NForm>
    </NCard>
  </div>
</template>
