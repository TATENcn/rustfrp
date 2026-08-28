<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import {
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NButton,
  NSpace,
  NSwitch,
  NSelect,
  useMessage,
  type FormRules,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useProxyStore } from '@/stores/proxies'
import { resolveErrorMessage } from '@/api/errors'
import type { LocalProxy, ProxyType } from '@/api/types'

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const message = useMessage()
const store = useProxyStore()
const props = withDefaults(defineProps<{ proxy?: LocalProxy | null; embedded?: boolean }>(), {
  proxy: null,
  embedded: false,
})
const emit = defineEmits<{ saved: [LocalProxy]; cancel: [] }>()

const isEdit = computed(() => !!props.proxy?.id || !!route.params.id)
const editId = computed(() => props.proxy?.id ?? Number(route.params.id))
const loading = ref(false)

const defaultProxy = (): LocalProxy => ({
  name: '',
  proxy_type: 'tcp',
  local_ip: '127.0.0.1',
  local_port: 0,
  remote_port: null,
  custom_domains: null,
  subdomain: null,
  use_encryption: true,
  use_compression: true,
  bandwidth_limit: null,
  bandwidth_limit_mode: null,
  secret_key: null,
  locations: null,
  http_user: null,
  http_password: null,
  host_header_rewrite: null,
  request_headers: null,
  response_headers: null,
  route_by_http_user: null,
  annotations: null,
  metadatas: null,
  allow_users: null,
  nat_traversal_disable_assisted_addrs: null,
  proxy_protocol_version: null,
  health_check_type: null,
  health_check_timeout_s: 3,
  health_check_max_failed: 3,
  health_check_interval_s: 10,
  health_check_path: null,
  health_check_http_headers: null,
  plugin_config: null,
  created_at: '',
  updated_at: '',
})

const form = ref<LocalProxy>(defaultProxy())

const typeOptions: { label: string; value: ProxyType }[] = [
  { label: 'TCP', value: 'tcp' },
  { label: 'UDP', value: 'udp' },
  { label: 'HTTP', value: 'http' },
  { label: 'HTTPS', value: 'https' },
  { label: 'STCP', value: 'stcp' },
  { label: 'XTCP', value: 'xtcp' },
  { label: 'TCPMUX', value: 'tcpmux' },
  { label: 'SUDP', value: 'sudp' },
]

const rules: FormRules = {
  name: [{ required: true, message: 'Name is required' }],
  local_ip: [{ required: true, message: 'Local IP is required' }],
  local_port: [{ required: true, type: 'number', min: 1, max: 65535 }],
}

// Helper: convert comma-separated string → string[] (null/empty stays null)
function splitList(val: unknown): string[] | null {
  if (!val) return null
  if (Array.isArray(val)) return val.length > 0 ? val : null
  const items = String(val).split(',').map(s => s.trim()).filter(Boolean)
  return items.length > 0 ? items : null
}

// Helper: convert string[] → comma-separated string for NInput display
function joinList(val: unknown): string {
  if (!val) return ''
  if (Array.isArray(val)) return val.join(', ')
  return String(val)
}

async function handleSubmit() {
  loading.value = true
  try {
    // Convert NInput string values to arrays before sending
    const payload = { ...form.value }
    payload.custom_domains = splitList(form.value.custom_domains)
    payload.locations = splitList(form.value.locations)
    payload.allow_users = splitList(form.value.allow_users)

    let saved: LocalProxy
    if (isEdit.value) {
      saved = await store.update(editId.value, payload)
      message.success(t('proxy.updateSuccess'))
    } else {
      saved = await store.create(payload)
      message.success(t('proxy.createSuccess'))
    }
    if (props.embedded) emit('saved', saved)
    else void router.push({ name: 'proxies' })
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    loading.value = false
  }
}

function loadProxy(proxy: LocalProxy) {
  form.value = {
    ...proxy,
    custom_domains: joinList(proxy.custom_domains) as any,
    locations: joinList(proxy.locations) as any,
    allow_users: joinList(proxy.allow_users) as any,
  }
}

function handleCancel() {
  if (props.embedded) emit('cancel')
  else void router.push({ name: 'proxies' })
}

onMounted(async () => {
  if (props.proxy) {
    loadProxy(props.proxy)
  } else if (isEdit.value) {
    await store.fetchAll()
    const existing = store.proxies.find((p) => p.id === editId.value)
    if (existing) loadProxy(existing)
  }
})
</script>

<template>
  <NSpace vertical>
    <h3 v-if="!embedded" style="margin: 0">
      {{ isEdit ? t('proxy.edit') : t('proxy.create') }}
    </h3>

    <NForm :model="form" :rules="rules">
      <NFormItem label="Name" path="name">
        <NInput v-model:value="form.name" />
      </NFormItem>
      <NFormItem :label="t('proxy.type')" path="proxy_type">
        <NSelect v-model:value="form.proxy_type" :options="typeOptions" />
      </NFormItem>
      <NFormItem :label="t('proxy.localIp')" path="local_ip">
        <NInput v-model:value="form.local_ip" placeholder="127.0.0.1" />
      </NFormItem>
      <NFormItem :label="t('proxy.localPort')" path="local_port">
        <NInputNumber v-model:value="form.local_port" :min="1" :max="65535" />
      </NFormItem>
      <NFormItem :label="t('proxy.remotePort')" path="remote_port">
        <NInputNumber v-model:value="form.remote_port" :min="1" :max="65535" />
      </NFormItem>

      <NFormItem :label="t('proxy.encryption')">
        <NSwitch v-model:value="form.use_encryption" />
      </NFormItem>
      <NFormItem :label="t('proxy.compression')">
        <NSwitch v-model:value="form.use_compression" />
      </NFormItem>

      <!-- HTTP fields (only relevant for http/https) -->
      <template v-if="form.proxy_type === 'http' || form.proxy_type === 'https'">
        <NFormItem label="Custom Domains">
          <NInput v-model:value="form.custom_domains" placeholder="Comma-separated" />
        </NFormItem>
        <NFormItem label="Subdomain">
          <NInput v-model:value="form.subdomain" />
        </NFormItem>
        <NFormItem label="Locations">
          <NInput v-model:value="form.locations" placeholder="Comma-separated" />
        </NFormItem>
        <NFormItem label="HTTP User">
          <NInput v-model:value="form.http_user" />
        </NFormItem>
        <NFormItem label="HTTP Password">
          <NInput v-model:value="form.http_password" type="password" />
        </NFormItem>
        <NFormItem label="Host Header Rewrite">
          <NInput v-model:value="form.host_header_rewrite" />
        </NFormItem>
      </template>

      <!-- Secret key (STCP/XTCP/SUDP) -->
      <template v-if="['stcp', 'xtcp', 'sudp'].includes(form.proxy_type)">
        <NFormItem label="Secret Key">
          <NInput v-model:value="form.secret_key" />
        </NFormItem>
      </template>

      <NSpace justify="end" style="margin-top: 16px">
        <NButton @click="handleCancel">
          {{ t('common.cancel') }}
        </NButton>
        <NButton type="primary" :loading="loading" @click="handleSubmit">
          {{ t('common.save') }}
        </NButton>
      </NSpace>
    </NForm>
  </NSpace>
</template>
