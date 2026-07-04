import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { LocalVisitor } from '@/api/types'
import * as visitorsApi from '@/api/visitors'
import { extractApiError } from '@/api/errors'

export const useVisitorStore = defineStore('visitors', () => {
  const visitors = ref<LocalVisitor[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchAll() {
    loading.value = true
    error.value = null
    try {
      const resp = await visitorsApi.listVisitors()
      visitors.value = resp.data ?? []
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function create(visitor: LocalVisitor): Promise<LocalVisitor> {
    const resp = await visitorsApi.createVisitor(visitor)
    if (resp.data) {
      visitors.value.push(resp.data)
    }
    return resp.data!
  }

  async function update(id: number, visitor: LocalVisitor): Promise<LocalVisitor> {
    const resp = await visitorsApi.updateVisitor(id, visitor)
    if (resp.data) {
      const idx = visitors.value.findIndex((v) => v.id === id)
      if (idx !== -1) visitors.value[idx] = resp.data
    }
    return resp.data!
  }

  async function remove(id: number) {
    await visitorsApi.deleteVisitor(id)
    visitors.value = visitors.value.filter((v) => v.id !== id)
  }

  return { visitors, loading, error, fetchAll, create, update, remove }
})
