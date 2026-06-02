import { useReducer, useCallback, useRef } from "react";
import type { ProfileSummary, ProfileDetail, MutationResult } from "../lib/types";
import * as api from "../lib/tauri-api";

interface State {
  profiles: ProfileSummary[];
  defaultProfile: string;
  selectedProfile: ProfileDetail | null;
  selectedName: string | null;
  loading: boolean;
  loadingHeavy: boolean;
  error: string | null;
  searchQuery: string;
}

type Action =
  | { type: "SET_PROFILES"; profiles: ProfileSummary[]; defaultProfile: string }
  | { type: "SELECT_PROFILE"; detail: ProfileDetail }
  | { type: "DESELECT" }
  | { type: "SET_LOADING"; loading: boolean; heavy?: boolean }
  | { type: "SET_ERROR"; error: string }
  | { type: "CLEAR_ERROR" }
  | { type: "SET_SEARCH"; query: string };

const initialState: State = {
  profiles: [],
  defaultProfile: "",
  selectedProfile: null,
  selectedName: null,
  loading: false,
  loadingHeavy: false,
  error: null,
  searchQuery: "",
};

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "SET_PROFILES":
      return {
        ...state,
        profiles: action.profiles,
        defaultProfile: action.defaultProfile,
        loading: false,
        loadingHeavy: false,
      };
    case "SELECT_PROFILE": {
      const tagsStr = action.detail.env?._KN_TAGS || "";
      const tags = tagsStr ? tagsStr.split(",").map((t: string) => t.trim()).filter((t: string) => t) : undefined;
      return {
        ...state,
        selectedProfile: { ...action.detail, tags },
        selectedName: action.detail.name,
        loading: false,
        loadingHeavy: false,
      };
    }
    case "DESELECT":
      return { ...state, selectedProfile: null, selectedName: null };
    case "SET_LOADING":
      return { ...state, loading: action.loading, loadingHeavy: action.heavy ?? false, error: null };
    case "SET_ERROR":
      return { ...state, error: action.error, loading: false, loadingHeavy: false };
    case "CLEAR_ERROR":
      return { ...state, error: null };
    case "SET_SEARCH":
      return { ...state, searchQuery: action.query };
    default:
      return state;
  }
}

export function useProfiles() {
  const [state, dispatch] = useReducer(reducer, initialState);
  const selectedNameRef = useRef<string | null>(null);
  selectedNameRef.current = state.selectedName;

  const loadProfiles = useCallback(async () => {
    dispatch({ type: "SET_LOADING", loading: true, heavy: true });
    try {
      const data = await api.listProfiles();
      dispatch({
        type: "SET_PROFILES",
        profiles: data.profiles,
        defaultProfile: data.default,
      });
    } catch (e) {
      dispatch({ type: "SET_ERROR", error: String(e) });
    }
  }, []);

  const selectProfile = useCallback(async (name: string) => {
    dispatch({ type: "SET_LOADING", loading: true, heavy: true });
    try {
      const detail = await api.showProfile(name);
      dispatch({ type: "SELECT_PROFILE", detail });
    } catch (e) {
      dispatch({ type: "SET_ERROR", error: String(e) });
    }
  }, []);

  const addProfile = useCallback(
    async (name: string, desc?: string) => {
      dispatch({ type: "SET_LOADING", loading: true, heavy: true });
      try {
        const result = await api.addProfile(name, desc);
        if (!result.ok && result.error) {
          dispatch({ type: "SET_ERROR", error: result.error });
          return result;
        }
        await loadProfiles();
        return result;
      } catch (e) {
        dispatch({ type: "SET_ERROR", error: String(e) });
        return { ok: false, error: String(e) };
      }
    },
    [loadProfiles]
  );

  const removeProfile = useCallback(
    async (name: string) => {
      dispatch({ type: "SET_LOADING", loading: true, heavy: true });
      try {
        await api.removeProfile(name);
        if (selectedNameRef.current === name) {
          dispatch({ type: "DESELECT" });
        }
        await loadProfiles();
      } catch (e) {
        dispatch({ type: "SET_ERROR", error: String(e) });
      }
    },
    [loadProfiles]
  );

  const setEnvVar = useCallback(
    async (profileName: string, key: string, value: string) => {
      dispatch({ type: "SET_LOADING", loading: true });
      try {
        await api.setEnvVar(profileName, key, value);
        // Refresh selected profile to show changes
        await selectProfile(profileName);
      } catch (e) {
        dispatch({ type: "SET_ERROR", error: String(e) });
      }
    },
    [selectProfile]
  );

  const unsetEnvVar = useCallback(
    async (profileName: string, key: string) => {
      dispatch({ type: "SET_LOADING", loading: true });
      try {
        await api.unsetEnvVar(profileName, key);
        await selectProfile(profileName);
      } catch (e) {
        dispatch({ type: "SET_ERROR", error: String(e) });
      }
    },
    [selectProfile]
  );

  const setDefault = useCallback(
    async (name: string) => {
      try {
        await api.setDefaultProfile(name);
        await loadProfiles();
      } catch (e) {
        dispatch({ type: "SET_ERROR", error: String(e) });
      }
    },
    [loadProfiles]
  );

  const initFromSettings = useCallback(async () => {
    dispatch({ type: "SET_LOADING", loading: true, heavy: true });
    try {
      const result = await api.initProfiles();
      if (!result.ok && result.error) {
        throw new Error(result.error);
      }
      await loadProfiles();
    } catch (e) {
      dispatch({ type: "SET_ERROR", error: String(e) });
      // Don't re-throw — error is already dispatched to state
    }
  }, [loadProfiles]);

  const search = useCallback((query: string) => {
    dispatch({ type: "SET_SEARCH", query });
  }, []);

  const clearError = useCallback(() => {
    dispatch({ type: "CLEAR_ERROR" });
  }, []);

  const deselect = useCallback(() => {
    dispatch({ type: "DESELECT" });
  }, []);

  const filteredProfiles = state.profiles.filter(
    (p) =>
      !state.searchQuery ||
      p.name.toLowerCase().includes(state.searchQuery.toLowerCase())
  );

  return {
    ...state,
    filteredProfiles,
    loadProfiles,
    selectProfile,
    addProfile,
    removeProfile,
    setEnvVar,
    unsetEnvVar,
    setDefault,
    initFromSettings,
    search,
    clearError,
    deselect,
  };
}
