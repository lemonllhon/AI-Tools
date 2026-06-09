import { useEffect } from 'react';
import { AlertCircle, CheckCircle2, RefreshCw, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useGlobalRefreshProgressStore } from '../stores/useGlobalRefreshProgressStore';
import './GlobalRefreshProgress.css';

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

export function GlobalRefreshProgress() {
  const { t } = useTranslation();
  const task = useGlobalRefreshProgressStore((state) => state.task);
  const clearTask = useGlobalRefreshProgressStore((state) => state.clearTask);

  useEffect(() => {
    if (!task || task.status === 'running') return;
    const timer = window.setTimeout(() => {
      clearTask();
    }, 6000);
    return () => window.clearTimeout(timer);
  }, [clearTask, task]);

  if (!task) return null;

  const percent = clampPercent(task.progress);
  const isRunning = task.status === 'running';
  const isSuccess = task.status === 'success';
  const countText =
    task.total > 0
      ? `${Math.min(task.completed, task.total)} / ${task.total}`
      : `${percent}%`;

  return (
    <div className={`global-refresh-progress global-refresh-progress--${task.status}`}>
      <div className="global-refresh-progress-icon" aria-hidden="true">
        {isRunning ? (
          <RefreshCw size={16} className="loading-spinner" />
        ) : isSuccess ? (
          <CheckCircle2 size={16} />
        ) : (
          <AlertCircle size={16} />
        )}
      </div>
      <div className="global-refresh-progress-main">
        <div className="global-refresh-progress-header">
          <span className="global-refresh-progress-title">{task.title}</span>
          <span className="global-refresh-progress-count">{countText}</span>
        </div>
        <div className="global-refresh-progress-message">
          {task.error || task.message || task.label}
        </div>
        <div
          className="global-refresh-progress-track"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
        >
          <div
            className="global-refresh-progress-fill"
            style={{ width: `${percent}%` }}
          />
        </div>
      </div>
      <button
        type="button"
        className="global-refresh-progress-close"
        onClick={clearTask}
        disabled={isRunning}
        aria-label={t('common.close', '关闭')}
        title={t('common.close', '关闭')}
      >
        <X size={14} />
      </button>
    </div>
  );
}
