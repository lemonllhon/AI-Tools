import { FormEvent, useEffect, useMemo, useState } from 'react';
import { AlertCircle, FileText, Plus, RefreshCw, Search, Trash2, X } from 'lucide-react';
import {
  applyProxyPoolImport,
  deleteProxyPoolNode,
  deleteProxyPoolNodes,
  listProxyPoolNodes,
  previewProxyPoolImport,
  saveProxyPoolNode,
  setProxyPoolNodeEnabled,
} from '../../services/proxyPoolService';
import type {
  ManualProxyNodeProtocol,
  ProxyImportPreviewResponse,
  ProxyPoolListResponse,
  ProxyPoolNode,
} from '../../types/proxyPool';
import { getCurrentLanguage } from '../../i18n';

interface ProxyNodeFormState {
  name: string;
  protocol: ManualProxyNodeProtocol;
  host: string;
  port: string;
  username: string;
  password: string;
  group: string;
  enabled: boolean;
}

const DEFAULT_FORM_STATE: ProxyNodeFormState = {
  name: '',
  protocol: 'http',
  host: '',
  port: '7890',
  username: '',
  password: '',
  group: '',
  enabled: true,
};

const MANUAL_PROTOCOLS: ManualProxyNodeProtocol[] = ['http', 'https', 'socks5'];

export function ProxyPoolSection() {
  const [data, setData] = useState<ProxyPoolListResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [form, setForm] = useState<ProxyNodeFormState>(DEFAULT_FORM_STATE);
  const [importContent, setImportContent] = useState('');
  const [importGroup, setImportGroup] = useState('');
  const [importNamePrefix, setImportNamePrefix] = useState('');
  const [importPreview, setImportPreview] = useState<ProxyImportPreviewResponse | null>(null);
  const [selectedPreviewIds, setSelectedPreviewIds] = useState<Set<string>>(() => new Set());
  const [previewLoading, setPreviewLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [search, setSearch] = useState('');
  const [groupFilter, setGroupFilter] = useState('all');
  const [protocolFilter, setProtocolFilter] = useState('all');
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const currentLanguage = getCurrentLanguage();

  const text = useMemo(() => {
    const isChinese = currentLanguage.toLowerCase().startsWith('zh');
    return isChinese
      ? {
          title: '代理节点池',
          desc: '当前阶段支持手动添加 http、https、socks5 节点；订阅、测速和桥接在后续阶段接入。',
          add: '添加节点',
          addResource: '添加资源',
          close: '收起',
          refresh: '刷新',
          search: '搜索名称、地址、分组',
          allGroups: '全部分组',
          allProtocols: '全部协议',
          deleteSelected: '删除所选',
          empty: '暂无匹配节点',
          loading: '加载中...',
          dbPath: '数据库',
          builtin: '内置',
          enabled: '启用',
          disabled: '停用',
          group: '分组',
          protocol: '协议',
          host: '地址',
          port: '端口',
          username: '账号',
          password: '密码',
          name: '名称',
          save: '保存节点',
          saving: '保存中...',
          optional: '可选',
          defaultGroup: '默认',
          confirmDelete: '确认删除这个代理节点？',
          confirmBatchDelete: '确认删除所选代理节点？',
          deleteFailed: '删除代理节点失败',
          saveFailed: '保存代理节点失败',
          loadFailed: '加载代理节点池失败',
          statusFailed: '更新代理节点状态失败',
          builtinLocked: '内置节点不能删除',
          directLocked: '直连节点不能禁用',
          passwordStored: '已保存密码',
          importTitle: '添加资源',
          importDesc: '粘贴 Clash YAML、Base64 订阅内容或分享链接；本阶段只解析并导入节点，不拉取远程订阅。',
          importContent: '资源内容',
          importContentPlaceholder: '粘贴 vmess://、vless://、trojan://、ss://、http://、socks5://、Base64 文本或 Clash YAML',
          namePrefix: '名称前缀',
          previewImport: '预览导入',
          previewing: '解析中...',
          applyImport: '导入所选',
          importing: '导入中...',
          importFailed: '导入代理资源失败',
          previewFailed: '解析代理资源失败',
          previewEmpty: '暂无可导入节点',
          parseWarnings: '解析提示',
          selectAll: '全选',
          importedCount: '已导入 {{count}} 个节点',
          previewCount: '{{count}} 个节点',
        }
      : {
          title: 'Proxy Node Pool',
          desc: 'This stage supports manually added http, https, and socks5 nodes. Subscriptions, tests, and bridging land later.',
          add: 'Add Node',
          addResource: 'Add Resource',
          close: 'Collapse',
          refresh: 'Refresh',
          search: 'Search name, address, group',
          allGroups: 'All groups',
          allProtocols: 'All protocols',
          deleteSelected: 'Delete selected',
          empty: 'No matching nodes',
          loading: 'Loading...',
          dbPath: 'Database',
          builtin: 'Built-in',
          enabled: 'Enabled',
          disabled: 'Disabled',
          group: 'Group',
          protocol: 'Protocol',
          host: 'Host',
          port: 'Port',
          username: 'Username',
          password: 'Password',
          name: 'Name',
          save: 'Save node',
          saving: 'Saving...',
          optional: 'Optional',
          defaultGroup: 'Default',
          confirmDelete: 'Delete this proxy node?',
          confirmBatchDelete: 'Delete selected proxy nodes?',
          deleteFailed: 'Failed to delete proxy node',
          saveFailed: 'Failed to save proxy node',
          loadFailed: 'Failed to load proxy node pool',
          statusFailed: 'Failed to update proxy node status',
          builtinLocked: 'Built-in nodes cannot be deleted',
          directLocked: 'Direct node cannot be disabled',
          passwordStored: 'Password saved',
          importTitle: 'Add Resource',
          importDesc: 'Paste Clash YAML, Base64 subscription text, or share links. This stage only parses and imports nodes.',
          importContent: 'Resource content',
          importContentPlaceholder: 'Paste vmess://, vless://, trojan://, ss://, http://, socks5://, Base64 text, or Clash YAML',
          namePrefix: 'Name prefix',
          previewImport: 'Preview import',
          previewing: 'Parsing...',
          applyImport: 'Import selected',
          importing: 'Importing...',
          importFailed: 'Failed to import proxy resource',
          previewFailed: 'Failed to parse proxy resource',
          previewEmpty: 'No importable nodes',
          parseWarnings: 'Parse notes',
          selectAll: 'Select all',
          importedCount: 'Imported {{count}} nodes',
          previewCount: '{{count}} nodes',
        };
  }, [currentLanguage]);

  const loadNodes = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await listProxyPoolNodes();
      setData(response);
      setSelectedIds((current) => {
        const validIds = new Set(response.nodes.filter((node) => !node.builtin).map((node) => node.id));
        return new Set(Array.from(current).filter((id) => validIds.has(id)));
      });
    } catch (err) {
      setError(`${text.loadFailed}: ${String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadNodes();
  }, []);

  const nodes = data?.nodes ?? [];
  const groups = data?.groups ?? [];

  const filteredNodes = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return nodes.filter((node) => {
      if (groupFilter !== 'all' && node.group !== groupFilter) return false;
      if (protocolFilter !== 'all' && node.protocol !== protocolFilter) return false;
      if (!needle) return true;
      return [node.name, node.protocol, node.group, node.maskedUrl, node.host]
        .join(' ')
        .toLowerCase()
        .includes(needle);
    });
  }, [nodes, search, groupFilter, protocolFilter]);

  const selectedDeletableIds = useMemo(
    () => nodes.filter((node) => selectedIds.has(node.id) && !node.builtin).map((node) => node.id),
    [nodes, selectedIds],
  );

  const updateForm = <K extends keyof ProxyNodeFormState>(key: K, value: ProxyNodeFormState[K]) => {
    setForm((current) => ({ ...current, [key]: value }));
  };

  const resetForm = () => {
    setForm(DEFAULT_FORM_STATE);
  };

  const resetImportPreview = () => {
    setImportPreview(null);
    setSelectedPreviewIds(new Set());
  };

  const handlePreviewImport = async () => {
    setPreviewLoading(true);
    setError(null);
    setNotice(null);
    try {
      const preview = await previewProxyPoolImport({
        content: importContent,
        group: importGroup || undefined,
        namePrefix: importNamePrefix || undefined,
      });
      setImportPreview(preview);
      setSelectedPreviewIds(new Set(preview.items.map((item) => item.previewId)));
    } catch (err) {
      setError(`${text.previewFailed}: ${String(err)}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  const handleApplyImport = async () => {
    if (selectedPreviewIds.size === 0) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      const result = await applyProxyPoolImport({
        content: importContent,
        group: importGroup || undefined,
        namePrefix: importNamePrefix || undefined,
        selectedPreviewIds: Array.from(selectedPreviewIds),
      });
      setData((current) => current ? { ...current, nodes: result.nodes } : current);
      setImportContent('');
      setImportGroup('');
      setImportNamePrefix('');
      resetImportPreview();
      setShowImport(false);
      await loadNodes();
      setNotice(text.importedCount.replace('{{count}}', String(result.imported)));
    } catch (err) {
      setError(`${text.importFailed}: ${String(err)}`);
    } finally {
      setImporting(false);
    }
  };

  const togglePreviewSelected = (previewId: string, selected: boolean) => {
    setSelectedPreviewIds((current) => {
      const next = new Set(current);
      if (selected) {
        next.add(previewId);
      } else {
        next.delete(previewId);
      }
      return next;
    });
  };

  const setAllPreviewSelected = (selected: boolean) => {
    setSelectedPreviewIds(selected ? new Set(importPreview?.items.map((item) => item.previewId) ?? []) : new Set());
  };

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const port = Number(form.port);
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      await saveProxyPoolNode({
        name: form.name,
        protocol: form.protocol,
        host: form.host,
        port,
        username: form.username || undefined,
        password: form.password || undefined,
        group: form.group || undefined,
        enabled: form.enabled,
      });
      resetForm();
      setShowForm(false);
      await loadNodes();
    } catch (err) {
      setError(`${text.saveFailed}: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (node: ProxyPoolNode) => {
    if (node.builtin) return;
    if (!window.confirm(text.confirmDelete)) return;
    setError(null);
    setNotice(null);
    try {
      await deleteProxyPoolNode(node.id);
      await loadNodes();
    } catch (err) {
      setError(`${text.deleteFailed}: ${String(err)}`);
    }
  };

  const handleDeleteSelected = async () => {
    if (selectedDeletableIds.length === 0) return;
    if (!window.confirm(text.confirmBatchDelete)) return;
    setError(null);
    setNotice(null);
    try {
      await deleteProxyPoolNodes(selectedDeletableIds);
      setSelectedIds(new Set());
      await loadNodes();
    } catch (err) {
      setError(`${text.deleteFailed}: ${String(err)}`);
    }
  };

  const handleToggleEnabled = async (node: ProxyPoolNode, enabled: boolean) => {
    if (node.id === '__direct__' && !enabled) return;
    setError(null);
    setNotice(null);
    try {
      const updated = await setProxyPoolNodeEnabled(node.id, enabled);
      setData((current) => {
        if (!current) return current;
        return {
          ...current,
          nodes: current.nodes.map((item) => (item.id === updated.id ? updated : item)),
        };
      });
    } catch (err) {
      setError(`${text.statusFailed}: ${String(err)}`);
    }
  };

  const toggleSelected = (node: ProxyPoolNode, selected: boolean) => {
    if (node.builtin) return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (selected) {
        next.add(node.id);
      } else {
        next.delete(node.id);
      }
      return next;
    });
  };

  return (
    <>
      <div className="group-title">{text.title}</div>
      <div className="settings-group proxy-pool-panel">
        <div className="proxy-pool-header">
          <div className="proxy-pool-copy">
            <div className="row-title">{text.title}</div>
            <div className="row-desc">{text.desc}</div>
          </div>
          <div className="proxy-pool-actions">
            <button className="btn btn-secondary" type="button" onClick={loadNodes} disabled={loading}>
              <RefreshCw size={16} className={loading ? 'animate-spin' : undefined} />
              {loading ? text.loading : text.refresh}
            </button>
            <button
              className="btn btn-secondary"
              type="button"
              onClick={() => {
                setShowImport((visible) => !visible);
                setShowForm(false);
              }}
            >
              <FileText size={16} />
              {text.addResource}
            </button>
            <button
              className="btn btn-primary"
              type="button"
              onClick={() => {
                setShowForm((visible) => !visible);
                setShowImport(false);
              }}
            >
              {showForm ? <X size={16} /> : <Plus size={16} />}
              {showForm ? text.close : text.add}
            </button>
          </div>
        </div>

        {error && (
          <div className="proxy-pool-error">
            <AlertCircle size={16} />
            <span>{error}</span>
          </div>
        )}

        {notice && (
          <div className="proxy-pool-notice">
            <span>{notice}</span>
          </div>
        )}

        {showImport && (
          <div className="proxy-pool-import">
            <div className="proxy-pool-import-head">
              <div>
                <div className="row-title">{text.importTitle}</div>
                <div className="row-desc">{text.importDesc}</div>
              </div>
            </div>
            <label className="proxy-pool-field proxy-pool-field--full">
              <span>{text.importContent}</span>
              <textarea
                className="settings-input proxy-pool-import-textarea"
                value={importContent}
                onChange={(event) => {
                  setImportContent(event.target.value);
                  resetImportPreview();
                }}
                placeholder={text.importContentPlaceholder}
              />
            </label>
            <div className="proxy-pool-import-options">
              <label className="proxy-pool-field">
                <span>{text.group}</span>
                <input
                  className="settings-input"
                  value={importGroup}
                  onChange={(event) => {
                    setImportGroup(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.defaultGroup}
                />
              </label>
              <label className="proxy-pool-field">
                <span>{text.namePrefix}</span>
                <input
                  className="settings-input"
                  value={importNamePrefix}
                  onChange={(event) => {
                    setImportNamePrefix(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.optional}
                />
              </label>
              <div className="proxy-pool-import-actions">
                <button
                  className="btn btn-secondary"
                  type="button"
                  onClick={handlePreviewImport}
                  disabled={previewLoading || !importContent.trim()}
                >
                  <RefreshCw size={16} className={previewLoading ? 'animate-spin' : undefined} />
                  {previewLoading ? text.previewing : text.previewImport}
                </button>
                <button
                  className="btn btn-primary"
                  type="button"
                  onClick={handleApplyImport}
                  disabled={importing || selectedPreviewIds.size === 0}
                >
                  {importing ? text.importing : text.applyImport}
                </button>
              </div>
            </div>

            {importPreview && (
              <div className="proxy-pool-preview">
                <div className="proxy-pool-preview-head">
                  <label className="proxy-pool-preview-select-all">
                    <input
                      type="checkbox"
                      checked={importPreview.items.length > 0 && selectedPreviewIds.size === importPreview.items.length}
                      disabled={importPreview.items.length === 0}
                      onChange={(event) => setAllPreviewSelected(event.target.checked)}
                    />
                    <span>{text.selectAll}</span>
                  </label>
                  <span>{text.previewCount.replace('{{count}}', String(importPreview.items.length))}</span>
                </div>
                {importPreview.errors.length > 0 && (
                  <div className="proxy-pool-preview-errors">
                    <strong>{text.parseWarnings}</strong>
                    {importPreview.errors.map((item, index) => (
                      <span key={`${item}-${index}`}>{item}</span>
                    ))}
                  </div>
                )}
                <div className="proxy-pool-preview-list">
                  {importPreview.items.length === 0 ? (
                    <div className="proxy-pool-empty">{text.previewEmpty}</div>
                  ) : (
                    importPreview.items.map((item) => (
                      <label className="proxy-pool-preview-item" key={item.previewId}>
                        <input
                          type="checkbox"
                          checked={selectedPreviewIds.has(item.previewId)}
                          onChange={(event) => togglePreviewSelected(item.previewId, event.target.checked)}
                        />
                        <div className="proxy-pool-preview-main">
                          <div className="proxy-pool-node-title">
                            <span>{item.name}</span>
                            <span className={`proxy-pool-protocol is-${item.protocol}`}>{item.protocol}</span>
                            <span className="proxy-pool-badge">{item.sourceKind}</span>
                          </div>
                          <code title={item.maskedUrl}>{item.maskedUrl}</code>
                          <div className="proxy-pool-node-meta">
                            <span>{text.group}: {item.group || '-'}</span>
                          </div>
                        </div>
                      </label>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        )}

        {showForm && (
          <form className="proxy-pool-form" onSubmit={handleSubmit}>
            <label className="proxy-pool-field">
              <span>{text.name}</span>
              <input
                className="settings-input"
                value={form.name}
                onChange={(event) => updateForm('name', event.target.value)}
                placeholder="My proxy"
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.protocol}</span>
              <select
                className="settings-select"
                value={form.protocol}
                onChange={(event) => updateForm('protocol', event.target.value as ManualProxyNodeProtocol)}
              >
                {MANUAL_PROTOCOLS.map((protocol) => (
                  <option key={protocol} value={protocol}>
                    {protocol}
                  </option>
                ))}
              </select>
            </label>
            <label className="proxy-pool-field proxy-pool-field--wide">
              <span>{text.host}</span>
              <input
                className="settings-input"
                value={form.host}
                onChange={(event) => updateForm('host', event.target.value)}
                placeholder="127.0.0.1"
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.port}</span>
              <input
                className="settings-input"
                type="number"
                min={1}
                max={65535}
                value={form.port}
                onChange={(event) => updateForm('port', event.target.value)}
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.username}</span>
              <input
                className="settings-input"
                value={form.username}
                onChange={(event) => updateForm('username', event.target.value)}
                placeholder={text.optional}
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.password}</span>
              <input
                className="settings-input"
                type="password"
                value={form.password}
                onChange={(event) => updateForm('password', event.target.value)}
                placeholder={text.optional}
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.group}</span>
              <input
                className="settings-input"
                value={form.group}
                onChange={(event) => updateForm('group', event.target.value)}
                placeholder={text.defaultGroup}
              />
            </label>
            <label className="proxy-pool-enabled">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => updateForm('enabled', event.target.checked)}
              />
              <span>{text.enabled}</span>
            </label>
            <div className="proxy-pool-form-actions">
              <button
                className="btn btn-secondary"
                type="button"
                onClick={() => {
                  resetForm();
                  setShowForm(false);
                }}
                disabled={saving}
              >
                {text.close}
              </button>
              <button className="btn btn-primary" type="submit" disabled={saving}>
                {saving ? text.saving : text.save}
              </button>
            </div>
          </form>
        )}

        <div className="proxy-pool-toolbar">
          <div className="proxy-pool-search">
            <Search size={15} />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={text.search}
            />
          </div>
          <select className="settings-select" value={groupFilter} onChange={(event) => setGroupFilter(event.target.value)}>
            <option value="all">{text.allGroups}</option>
            {groups.map((group) => (
              <option key={group} value={group}>
                {group}
              </option>
            ))}
          </select>
          <select
            className="settings-select"
            value={protocolFilter}
            onChange={(event) => setProtocolFilter(event.target.value)}
          >
            <option value="all">{text.allProtocols}</option>
            <option value="direct">direct</option>
            {MANUAL_PROTOCOLS.map((protocol) => (
              <option key={protocol} value={protocol}>
                {protocol}
              </option>
            ))}
          </select>
          <button
            className="btn btn-secondary proxy-pool-delete-selected"
            type="button"
            onClick={handleDeleteSelected}
            disabled={selectedDeletableIds.length === 0}
          >
            <Trash2 size={16} />
            {text.deleteSelected}
          </button>
        </div>

        <div className="proxy-pool-list">
          {loading && nodes.length === 0 ? (
            <div className="proxy-pool-empty">{text.loading}</div>
          ) : filteredNodes.length === 0 ? (
            <div className="proxy-pool-empty">{text.empty}</div>
          ) : (
            filteredNodes.map((node) => (
              <div className="proxy-pool-node" key={node.id}>
                <label className="proxy-pool-node-select" title={node.builtin ? text.builtinLocked : ''}>
                  <input
                    type="checkbox"
                    checked={selectedIds.has(node.id)}
                    disabled={node.builtin}
                    onChange={(event) => toggleSelected(node, event.target.checked)}
                  />
                </label>
                <div className="proxy-pool-node-main">
                  <div className="proxy-pool-node-title">
                    <span>{node.name}</span>
                    <span className={`proxy-pool-protocol is-${node.protocol}`}>{node.protocol}</span>
                    {node.builtin && <span className="proxy-pool-badge">{text.builtin}</span>}
                  </div>
                  <code title={node.maskedUrl}>{node.maskedUrl}</code>
                  <div className="proxy-pool-node-meta">
                    <span>{text.group}: {node.group || '-'}</span>
                    {node.hasPassword && <span>{text.passwordStored}</span>}
                  </div>
                </div>
                <div className="proxy-pool-node-state">
                  <label className="switch" title={node.id === '__direct__' ? text.directLocked : ''}>
                    <input
                      type="checkbox"
                      checked={node.enabled}
                      disabled={node.id === '__direct__'}
                      onChange={(event) => void handleToggleEnabled(node, event.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                  <span className={`proxy-pool-state-text ${node.enabled ? 'is-enabled' : 'is-disabled'}`}>
                    {node.enabled ? text.enabled : text.disabled}
                  </span>
                </div>
                <button
                  className="proxy-pool-icon-btn"
                  type="button"
                  onClick={() => void handleDelete(node)}
                  disabled={node.builtin}
                  title={node.builtin ? text.builtinLocked : text.deleteSelected}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            ))
          )}
        </div>

        {data?.dbPath && (
          <div className="proxy-pool-db-path" title={data.dbPath}>
            <span>{text.dbPath}</span>
            <code>{data.dbPath}</code>
          </div>
        )}
      </div>
    </>
  );
}
