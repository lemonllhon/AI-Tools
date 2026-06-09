import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { AlertCircle, CheckCircle2, RefreshCw, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useGlobalRefreshProgressStore } from '../stores/useGlobalRefreshProgressStore';
import './GlobalRefreshProgress.css';

const POSITION_STORAGE_KEY = 'ai-lemon.global-refresh-progress.position.v1';
const EDGE_MARGIN = 12;

interface RefreshProgressPosition {
  left: number;
  top: number;
}

interface DragState {
  pointerId: number;
  offsetX: number;
  offsetY: number;
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

function readSavedPosition(): RefreshProgressPosition | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(POSITION_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<RefreshProgressPosition>;
    if (!Number.isFinite(parsed.left) || !Number.isFinite(parsed.top)) {
      return null;
    }
    return {
      left: Number(parsed.left),
      top: Number(parsed.top),
    };
  } catch {
    return null;
  }
}

function savePosition(position: RefreshProgressPosition): void {
  try {
    window.localStorage.setItem(POSITION_STORAGE_KEY, JSON.stringify(position));
  } catch {
    // Ignore storage failures; dragging should still work for the current session.
  }
}

function clampPosition(
  left: number,
  top: number,
  width: number,
  height: number,
): RefreshProgressPosition {
  const maxLeft = Math.max(EDGE_MARGIN, window.innerWidth - width - EDGE_MARGIN);
  const maxTop = Math.max(EDGE_MARGIN, window.innerHeight - height - EDGE_MARGIN);
  return {
    left: Math.min(Math.max(EDGE_MARGIN, left), maxLeft),
    top: Math.min(Math.max(EDGE_MARGIN, top), maxTop),
  };
}

export function GlobalRefreshProgress() {
  const { t } = useTranslation();
  const task = useGlobalRefreshProgressStore((state) => state.task);
  const clearTask = useGlobalRefreshProgressStore((state) => state.clearTask);
  const progressRef = useRef<HTMLDivElement | null>(null);
  const dragStateRef = useRef<DragState | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [position, setPosition] = useState<RefreshProgressPosition | null>(
    readSavedPosition,
  );

  const clampCurrentPosition = useCallback(
    (nextPosition: RefreshProgressPosition): RefreshProgressPosition => {
      const rect = progressRef.current?.getBoundingClientRect();
      return clampPosition(
        nextPosition.left,
        nextPosition.top,
        rect?.width ?? 380,
        rect?.height ?? 90,
      );
    },
    [],
  );

  const setClampedPosition = useCallback(
    (nextPosition: RefreshProgressPosition) => {
      const clamped = clampCurrentPosition(nextPosition);
      setPosition(clamped);
      savePosition(clamped);
    },
    [clampCurrentPosition],
  );

  useEffect(() => {
    if (!task || task.status === 'running') return;
    const timer = window.setTimeout(() => {
      clearTask();
    }, 6000);
    return () => window.clearTimeout(timer);
  }, [clearTask, task]);

  useEffect(() => {
    if (!task || !position) return;
    const frame = window.requestAnimationFrame(() => {
      setPosition((prev) => {
        if (!prev) return prev;
        const next = clampCurrentPosition(prev);
        if (next.left === prev.left && next.top === prev.top) return prev;
        savePosition(next);
        return next;
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [clampCurrentPosition, position?.left, position?.top, task?.id]);

  useEffect(() => {
    const handleResize = () => {
      setPosition((prev) => {
        if (!prev) return prev;
        const next = clampCurrentPosition(prev);
        if (next.left === prev.left && next.top === prev.top) return prev;
        savePosition(next);
        return next;
      });
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [clampCurrentPosition]);

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('button, a, input, textarea, select')) return;

      const rect = progressRef.current?.getBoundingClientRect();
      if (!rect) return;

      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      dragStateRef.current = {
        pointerId: event.pointerId,
        offsetX: event.clientX - rect.left,
        offsetY: event.clientY - rect.top,
      };
      setIsDragging(true);
      setClampedPosition({ left: rect.left, top: rect.top });
    },
    [setClampedPosition],
  );

  const handlePointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const dragState = dragStateRef.current;
      if (!dragState || dragState.pointerId !== event.pointerId) return;
      event.preventDefault();
      setClampedPosition({
        left: event.clientX - dragState.offsetX,
        top: event.clientY - dragState.offsetY,
      });
    },
    [setClampedPosition],
  );

  const stopDragging = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const dragState = dragStateRef.current;
    if (!dragState || dragState.pointerId !== event.pointerId) return;
    dragStateRef.current = null;
    setIsDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }, []);

  if (!task) return null;

  const percent = clampPercent(task.progress);
  const isRunning = task.status === 'running';
  const isSuccess = task.status === 'success';
  const countText =
    task.total > 0
      ? `${Math.min(task.completed, task.total)} / ${task.total}`
      : `${percent}%`;
  const positionStyle: CSSProperties | undefined = position
    ? {
        left: position.left,
        top: position.top,
        right: 'auto',
        bottom: 'auto',
      }
    : undefined;
  const className = [
    'global-refresh-progress',
    `global-refresh-progress--${task.status}`,
    position ? 'global-refresh-progress--positioned' : '',
    isDragging ? 'global-refresh-progress--dragging' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div
      ref={progressRef}
      className={className}
      style={positionStyle}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
    >
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
