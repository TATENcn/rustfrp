<script setup lang="ts">
import { NModal, NCard, NButton, NSpace } from 'naive-ui'
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
  <NModal
    :show="show"
    :mask-closable="!loading"
    @update:show="value => { if (!value && !loading) emit('cancel') }"
  >
    <NCard
      :title="title"
      :bordered="false"
      role="alertdialog"
      aria-modal="true"
      style="width: min(440px, calc(100vw - 32px))"
    >
      <p style="margin: 0">{{ content }}</p>
      <template #footer>
        <NSpace justify="end">
          <NButton :disabled="loading" @click="emit('cancel')">{{ t('common.cancel') }}</NButton>
          <NButton
            type="error"
            :loading="loading"
            @click="emit('confirm')"
          >
            {{ t('common.delete') }}
          </NButton>
        </NSpace>
      </template>
    </NCard>
  </NModal>
</template>
