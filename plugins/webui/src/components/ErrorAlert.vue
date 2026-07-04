<script setup lang="ts">
import { NAlert } from 'naive-ui'

defineProps<{
  error: string | null
}>()

const emit = defineEmits<{
  dismiss: []
}>()
</script>

<template>
  <NAlert
    v-if="error"
    type="error"
    closable
    style="margin-bottom: 16px"
    @close="emit('dismiss')"
  >
    {{ error }}
  </NAlert>
</template>
