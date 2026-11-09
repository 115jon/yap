/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `vite dev` only: the "Yap Admin" Kova key for the sign-in page. */
  readonly VITE_KOVA_ADMIN_PUBLISHABLE_KEY?: string;
}
