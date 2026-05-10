import axios from 'axios'
import type {
  BusinessTerm, Datasource, FewShot, Message, MetricDef, Session, Uuid,
} from './types'

export const http = axios.create({
  baseURL: '/',
  timeout: 30000,
})

// ----- sessions -----
export async function createSession(p: { user_id: string; datasource_id: Uuid; title?: string }) {
  const r = await http.post<Session>('/api/sessions', p)
  return r.data
}
export async function listMessages(session_id: Uuid) {
  const r = await http.get<Message[]>(`/api/sessions/${session_id}/messages`)
  return r.data
}

// ----- admin: datasource -----
export async function listDatasources() {
  const r = await http.get<Datasource[]>('/api/admin/datasources')
  return r.data
}
export async function createDatasource(p: Partial<Datasource> & { name: string; kind: string; host: string; port: number; db: string; user_name: string; secret_ref: string }) {
  const r = await http.post<Datasource>('/api/admin/datasources', p)
  return r.data
}

// ----- admin: business term -----
export async function listTerms() {
  const r = await http.get<BusinessTerm[]>('/api/admin/business-terms')
  return r.data
}
export async function upsertTerm(p: { term: string; aliases: string[]; definition: string; formula?: string }) {
  const r = await http.post<BusinessTerm>('/api/admin/business-terms', p)
  return r.data
}
export async function deleteTerm(term: string) {
  return http.delete(`/api/admin/business-terms/${encodeURIComponent(term)}`)
}

// ----- admin: metric -----
export async function listMetrics() {
  const r = await http.get<MetricDef[]>('/api/admin/metrics')
  return r.data
}
export async function upsertMetric(p: { name: string; dimension_keys: string[]; measure_sql: string; owner?: string }) {
  const r = await http.post<MetricDef>('/api/admin/metrics', p)
  return r.data
}
export async function deleteMetric(name: string) {
  return http.delete(`/api/admin/metrics/${encodeURIComponent(name)}`)
}

// ----- admin: few-shot -----
export async function listFewShots() {
  const r = await http.get<FewShot[]>('/api/admin/few-shots')
  return r.data
}
export async function createFewShot(p: { question: string; skill_call: any; sql_text: string; datasource_id?: Uuid }) {
  const r = await http.post<FewShot>('/api/admin/few-shots', p)
  return r.data
}
export async function voteFewShot(id: Uuid, delta: number) {
  const r = await http.post<FewShot>(`/api/admin/few-shots/${id}/vote`, { delta })
  return r.data
}
export async function deleteFewShot(id: Uuid) {
  return http.delete(`/api/admin/few-shots/${id}`)
}

export function csvExportUrl(message_id: Uuid): string {
  return `/api/messages/${message_id}/export.csv`
}
