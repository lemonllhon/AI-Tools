import { create } from 'zustand';
import type { TopRightAdState } from '../types/topRightAd';
import { getTopRightAdState } from '../services/topRightAdService';

const EMPTY_STATE: TopRightAdState = {
  ad: null,
};

interface TopRightAdStoreState {
  state: TopRightAdState;
  loading: boolean;
  initialized: boolean;
  fetchState: () => Promise<TopRightAdState>;
}

export const useTopRightAdStore = create<TopRightAdStoreState>((set) => ({
  state: EMPTY_STATE,
  loading: false,
  initialized: false,

  fetchState: async () => {
    set({ loading: true });
    try {
      const nextState = await getTopRightAdState();
      set({ state: nextState, loading: false, initialized: true });
      return nextState;
    } catch (error) {
      console.error('加载欢迎信息失败:', error);
      set({ state: EMPTY_STATE, loading: false, initialized: true });
      return EMPTY_STATE;
    }
  },
}));
