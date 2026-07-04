<script setup lang="ts">
import { NModal, NButton, NSpace } from 'naive-ui'
import { useI18n } from '@/i18n'

const props = defineProps<{
  show: boolean
  title: string
  content: string
  loading?: boolean
}>()

const emit = defineEmits<{
  confirm: []
  cancel: []
}>()

const { t } = useI18n()
</script>

<template>
  <NModal :show="show" :title="title" @update:show="emit('cancel')">
    <template #header>{{ title }}</template>
    <p>{{ content }}</p>
    <template #footer>
      <NSpace justify="end">
        <NButton @click="emit('cancel')">{{ t('common.cancel') }}</NButton>
        <NButton
          type="error"
          :loading="loading"
          @click="emit('confirm')"
        >
          {{ t('common.delete') }}
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>
