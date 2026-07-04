import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { BindingRule, BindingControlResponse } from '@/api/types'
import * as bindingsApi from '@/api/bindings'
import { extractApiError } from '@/api/errors'

export const useBindingStore = defineStore('bindings', () => {
  const bindings = ref<BindingRule[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchAll() {
    loading.value = true
    error.value = null
    try {
      const resp = await bindingsApi.listBindings()
      bindings.value = resp.data ?? []
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function create(binding: BindingRule): Promise<BindingRule> {
    const resp = await bindingsApi.createBinding(binding)
    if (resp.data) {
      bindings.value.push(resp.data)
    }
    return resp.data!
  }

  async function update(id: number, binding: BindingRule): Promise<BindingRule> {
    const resp = await bindingsApi.updateBinding(id, binding)
    if (resp.data) {
      const idx = bindings.value.findIndex((b) => b.id === id)
      if (idx !== -1) bindings.value[idx] = resp.data
    }
    return resp.data!
  }

  async function toggle(id: number, enabled: boolean): Promise<BindingRule> {
    const resp = await bindingsApi.toggleBinding(id, enabled)
    if (resp.data) {
      const idx = bindings.value.findIndex((b) => b.id === id)
      if (idx !== -1) bindings.value[idx] = resp.data
    }
    return resp.data!
  }

  async function startBinding(id: number): Promise<BindingControlResponse> {
    const resp = await bindingsApi.startBinding(id)
    if (resp.data) {
      // Update local binding state
      const idx = bindings.value.findIndex((b) => b.id === id)
      if (idx !== -1) {
        bindings.value[idx] = { ...bindings.value[idx], running: true }
      }
    }
    return resp.data!
  }

  async function stopBinding(id: number): Promise<BindingControlResponse> {
    const resp = await bindingsApi.stopBinding(id)
    if (resp.data) {
      // Update local binding state
      const idx = bindings.value.findIndex((b) => b.id === id)
      if (idx !== -1) {
        bindings.value[idx] = { ...bindings.value[idx], running: false }
      }
    }
    return resp.data!
  }

  async function remove(id: number) {
    await bindingsApi.deleteBinding(id)
    bindings.value = bindings.value.filter((b) => b.id !== id)
  }

  return { bindings, loading, error, fetchAll, create, update, toggle, startBinding, stopBinding, remove }
})
