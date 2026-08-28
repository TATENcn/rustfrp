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
import { useProfileStore } from '@/stores/profiles'
import { resolveErrorMessage } from '@/api/errors'
import type { FrpsProfile } from '@/api/types'

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const message = useMessage()
const store = useProfileStore()
const props = withDefaults(defineProps<{ profile?: FrpsProfile | null; embedded?: boolean }>(), {
  profile: null,
  embedded: false,
})
const emit = defineEmits<{ saved: [FrpsProfile]; cancel: [] }>()

const isEdit = computed(() => !!props.profile?.id || !!route.params.id)
const editId = computed(() => props.profile?.id ?? Number(route.params.id))
const loading = ref(false)

const defaultProfile = (): FrpsProfile => ({
  name: '',
  server_addr: '',
  server_port: 7000,
  tls_enable: false,
  tls_cert_file: null,
  tls_key_file: null,
  tls_trusted_ca_file: null,
  transport_protocol: 'tcp',
  heartbeat_interval: 30,
  heartbeat_timeout: 90,
  dial_server_timeout: null,
  dial_server_keepalive: null,
  connect_server_local_ip: null,
  proxy_url: null,
  pool_count: null,
  tcp_mux: null,
  tcp_mux_keepalive_interval: null,
  quic_keepalive_period: null,
  quic_max_idle_timeout: null,
  quic_max_incoming_streams: null,
  auth_method: 'none',
  oidc_client_id: null,
  oidc_client_secret: null,
  oidc_token_endpoint_url: null,
  oidc_audience: null,
  oidc_scope: null,
  oidc_additional_endpoint_params: null,
  user: null,
  metadatas: null,
  login_fail_exit: null,
  start: null,
  dns_server: null,
  nat_hole_stun_server: null,
  udp_packet_size: null,
  includes: null,
  store_path: null,
  feature_gates: null,
  created_at: '',
  updated_at: '',
})

const form = ref<FrpsProfile>(defaultProfile())

const transportOptions = [
  { label: 'TCP', value: 'tcp' },
  { label: 'KCP', value: 'kcp' },
  { label: 'WebSocket', value: 'wss' },
  { label: 'QUIC', value: 'quic' },
]

const authOptions = [
  { label: t('profile.authNone'), value: 'none' },
  { label: 'Token', value: 'token' },
  { label: 'OIDC', value: 'oidc' },
]

const rules: FormRules = {
  name: [{ required: true, message: 'Name is required' }],
  server_addr: [{ required: true, message: 'Server address is required' }],
  server_port: [{ required: true, type: 'number', min: 1, max: 65535 }],
}

async function handleSubmit() {
  loading.value = true
  try {
    let saved: FrpsProfile
    if (isEdit.value) {
      saved = await store.update(editId.value, form.value)
      message.success(t('profile.updateSuccess'))
    } else {
      saved = await store.create(form.value)
      message.success(t('profile.createSuccess'))
    }
    if (props.embedded) emit('saved', saved)
    else void router.push({ name: 'profiles' })
  } catch (e: any) {
    message.error(t(resolveErrorMessage(e?.code)))
  } finally {
    loading.value = false
  }
}

function handleCancel() {
  if (props.embedded) emit('cancel')
  else void router.push({ name: 'profiles' })
}

onMounted(async () => {
  if (props.profile) {
    form.value = { ...props.profile }
    form.value.auth_method ??= 'token'
  } else if (isEdit.value) {
    await store.fetchAll()
    const existing = store.profiles.find((p) => p.id === editId.value)
    if (existing) {
      form.value = { ...existing }
      form.value.auth_method ??= 'token'
    }
  }
})
</script>

<template>
  <NSpace vertical>
    <h3 v-if="!embedded" style="margin: 0">
      {{ isEdit ? t('profile.edit') : t('profile.create') }}
    </h3>

    <NForm :model="form" :rules="rules">
      <!-- Basic -->
      <NFormItem label="Name" path="name">
        <NInput v-model:value="form.name" />
      </NFormItem>
      <NFormItem label="Server Address" path="server_addr">
        <NInput v-model:value="form.server_addr" placeholder="frp.example.com" />
      </NFormItem>
      <NFormItem label="Server Port" path="server_port">
        <NInputNumber v-model:value="form.server_port" :min="1" :max="65535" />
      </NFormItem>
      <NFormItem label="Transport Protocol">
        <NSelect v-model:value="form.transport_protocol" :options="transportOptions" />
      </NFormItem>

      <!-- Auth -->
      <NFormItem label="Auth Method">
        <NSelect
          v-model:value="form.auth_method"
          :options="authOptions"
          :placeholder="t('profile.authPlaceholder')"
        />
      </NFormItem>
      <NFormItem
        v-if="form.auth_method === 'token'"
        :label="t('profile.token')"
        path="token"
      >
        <NInput
          v-model:value="form.token"
          type="password"
          show-password-on="click"
          :placeholder="isEdit ? t('profile.tokenKeep') : ''"
        />
      </NFormItem>
      <template v-if="form.auth_method === 'oidc'">
        <NFormItem label="OIDC Client ID">
          <NInput v-model:value="form.oidc_client_id" />
        </NFormItem>
        <NFormItem label="OIDC Client Secret">
          <NInput v-model:value="form.oidc_client_secret" type="password" show-password-on="click" />
        </NFormItem>
        <NFormItem label="OIDC Token Endpoint URL">
          <NInput v-model:value="form.oidc_token_endpoint_url" placeholder="https://issuer.example.com/oauth/token" />
        </NFormItem>
        <NFormItem label="OIDC Audience">
          <NInput v-model:value="form.oidc_audience" />
        </NFormItem>
        <NFormItem label="OIDC Scope">
          <NInput v-model:value="form.oidc_scope" />
        </NFormItem>
      </template>

      <!-- TLS -->
      <NFormItem label="TLS Enable">
        <NSwitch v-model:value="form.tls_enable" />
      </NFormItem>
      <template v-if="form.tls_enable">
        <NFormItem label="TLS Cert File">
          <NInput v-model:value="form.tls_cert_file" />
        </NFormItem>
        <NFormItem label="TLS Key File">
          <NInput v-model:value="form.tls_key_file" />
        </NFormItem>
        <NFormItem label="TLS Trusted CA File">
          <NInput v-model:value="form.tls_trusted_ca_file" />
        </NFormItem>
      </template>

      <!-- Advanced -->
      <NFormItem label="Heartbeat Interval (s)">
        <NInputNumber v-model:value="form.heartbeat_interval" :min="1" />
      </NFormItem>
      <NFormItem label="Heartbeat Timeout (s)">
        <NInputNumber v-model:value="form.heartbeat_timeout" :min="1" />
      </NFormItem>
      <NFormItem label="User (prefix)">
        <NInput v-model:value="form.user" />
      </NFormItem>
      <NFormItem label="Login Fail Exit">
        <NSwitch v-model:value="form.login_fail_exit" />
      </NFormItem>

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
