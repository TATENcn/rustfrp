import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { LocalProxy } from '@/api/types'
import * as proxiesApi from '@/api/proxies'
import { extractApiError } from '@/api/errors'

export const useProxyStore = defineStore('proxies', () => {
  const proxies = ref<LocalProxy[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchAll() {
    loading.value = true
    error.value = null
    try {
      const resp = await proxiesApi.listProxies()
      proxies.value = resp.data ?? []
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function create(proxy: LocalProxy): Promise<LocalProxy> {
    const resp = await proxiesApi.createProxy(proxy)
    if (resp.data) {
      proxies.value.push(resp.data)
    }
    return resp.data!
  }

  async function update(id: number, proxy: LocalProxy): Promise<LocalProxy> {
    const resp = await proxiesApi.updateProxy(id, proxy)
    if (resp.data) {
      const idx = proxies.value.findIndex((p) => p.id === id)
      if (idx !== -1) proxies.value[idx] = resp.data
    }
    return resp.data!
  }

  async function remove(id: number) {
    await proxiesApi.deleteProxy(id)
    proxies.value = proxies.value.filter((p) => p.id !== id)
  }

  return { proxies, loading, error, fetchAll, create, update, remove }
})
