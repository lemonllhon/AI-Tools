import { create } from 'zustand';

export type GlobalRefreshProgressStatus = 'running' | 'success' | 'error';

export interface GlobalRefreshProgressTask {
  id: string;
  platformId: string;
  label: string;
  title: string;
  status: GlobalRefreshProgressStatus;
  total: number;
  completed: number;
  progress: number;
  message: string;
  error?: string;
  startedAt: number;
  updatedAt: number;
}

interface StartRefreshProgressTaskInput {
  platformId: string;
  label: string;
  title: string;
  total?: number;
  message?: string;
  autoPulse?: boolean;
}

interface FinishRefreshProgressTaskInput {
  status: Exclude<GlobalRefreshProgressStatus, 'running'>;
  completed?: number;
  message?: string;
  error?: string;
}

interface GlobalRefreshProgressState {
  task: GlobalRefreshProgressTask | null;
  startTask: (input: StartRefreshProgressTaskInput) => string;
  updateTask: (
    taskId: string,
    patch: Partial<
      Pick<GlobalRefreshProgressTask, 'completed' | 'total' | 'progress' | 'message'>
    >,
  ) => void;
  finishTask: (taskId: string, input: FinishRefreshProgressTaskInput) => void;
  clearTask: () => void;
}

let refreshProgressPulseTimer: ReturnType<typeof setInterval> | null = null;
let refreshProgressSequence = 0;

function clampProgress(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

function stopPulseTimer(): void {
  if (refreshProgressPulseTimer) {
    clearInterval(refreshProgressPulseTimer);
    refreshProgressPulseTimer = null;
  }
}

function nextPulseProgress(current: number): number {
  if (current < 18) return current + 4;
  if (current < 45) return current + 3;
  if (current < 75) return current + 2;
  return Math.min(92, current + 1);
}

export const useGlobalRefreshProgressStore = create<GlobalRefreshProgressState>(
  (set, get) => ({
    task: null,
    startTask: (input) => {
      stopPulseTimer();
      refreshProgressSequence += 1;
      const taskId = `${Date.now()}-${refreshProgressSequence}`;
      const now = Date.now();
      set({
        task: {
          id: taskId,
          platformId: input.platformId,
          label: input.label,
          title: input.title,
          status: 'running',
          total: Math.max(0, Math.floor(input.total ?? 0)),
          completed: 0,
          progress: 4,
          message: input.message ?? '',
          startedAt: now,
          updatedAt: now,
        },
      });

      if (input.autoPulse !== false) {
        refreshProgressPulseTimer = setInterval(() => {
          set((state) => {
            if (!state.task || state.task.id !== taskId || state.task.status !== 'running') {
              return state;
            }
            return {
              task: {
                ...state.task,
                progress: clampProgress(nextPulseProgress(state.task.progress)),
                updatedAt: Date.now(),
              },
            };
          });
        }, 800);
      }

      return taskId;
    },
    updateTask: (taskId, patch) => {
      set((state) => {
        if (!state.task || state.task.id !== taskId) return state;
        const total =
          patch.total === undefined
            ? state.task.total
            : Math.max(0, Math.floor(patch.total));
        const completed =
          patch.completed === undefined
            ? state.task.completed
            : Math.max(0, Math.floor(patch.completed));
        const progress =
          patch.progress !== undefined
            ? clampProgress(patch.progress)
            : total > 0
              ? clampProgress((completed / total) * 100)
              : state.task.progress;
        return {
          task: {
            ...state.task,
            ...patch,
            total,
            completed,
            progress,
            updatedAt: Date.now(),
          },
        };
      });
    },
    finishTask: (taskId, input) => {
      const current = get().task;
      if (!current || current.id !== taskId) return;
      stopPulseTimer();
      set({
        task: {
          ...current,
          status: input.status,
          completed: input.completed ?? current.total,
          progress: 100,
          message: input.message ?? current.message,
          error: input.error,
          updatedAt: Date.now(),
        },
      });
    },
    clearTask: () => {
      stopPulseTimer();
      set({ task: null });
    },
  }),
);
