import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter, Route, Routes, Navigate } from 'react-router-dom'
import App from './App'
import Chat from './pages/Chat'
import Admin from './pages/Admin'
import { DatasourceTab } from './components/admin/DatasourceTab'
import { TermTab } from './components/admin/TermTab'
import { MetricTab } from './components/admin/MetricTab'
import { FewShotTab } from './components/admin/FewShotTab'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<App />}>
          <Route index element={<Navigate to="/chat" replace />} />
          <Route path="chat" element={<Chat />} />
          <Route path="admin" element={<Admin />}>
            <Route index element={<DatasourceTab />} />
            <Route path="datasources" element={<DatasourceTab />} />
            <Route path="terms" element={<TermTab />} />
            <Route path="metrics" element={<MetricTab />} />
            <Route path="few-shots" element={<FewShotTab />} />
          </Route>
        </Route>
      </Routes>
    </BrowserRouter>
  </React.StrictMode>,
)
