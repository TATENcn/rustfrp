import { createRouter, createWebHashHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: () => import('@/views/LoginPage.vue'),
    meta: { noAuth: true },
  },
  {
    path: '/',
    component: () => import('@/components/AppLayout.vue'),
    children: [
      {
        path: '',
        name: 'dashboard',
        component: () => import('@/views/Dashboard.vue'),
      },
      {
        path: 'profiles',
        name: 'profiles',
        component: () => import('@/views/profiles/ProfileList.vue'),
      },
      {
        path: 'profiles/new',
        name: 'profile-new',
        component: () => import('@/views/profiles/ProfileForm.vue'),
      },
      {
        path: 'profiles/:id',
        name: 'profile-edit',
        component: () => import('@/views/profiles/ProfileForm.vue'),
      },
      {
        path: 'proxies',
        name: 'proxies',
        component: () => import('@/views/proxies/ProxyList.vue'),
      },
      {
        path: 'proxies/new',
        name: 'proxy-new',
        component: () => import('@/views/proxies/ProxyForm.vue'),
      },
      {
        path: 'proxies/:id',
        name: 'proxy-edit',
        component: () => import('@/views/proxies/ProxyForm.vue'),
      },
      {
        path: 'bindings',
        name: 'bindings',
        component: () => import('@/views/bindings/BindingList.vue'),
      },
      {
        path: 'visitors',
        name: 'visitors',
        component: () => import('@/views/visitors/VisitorList.vue'),
      },
      {
        path: 'logs',
        name: 'logs',
        component: () => import('@/views/logs/LogViewer.vue'),
      },
      {
        path: 'status',
        name: 'status',
        component: () => import('@/views/SystemStatus.vue'),
      },
    ],
  },
]

const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

export default router
