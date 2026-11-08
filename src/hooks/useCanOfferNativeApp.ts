import { canOfferNativeApp } from "@/lib/native-app-link";
import { useSyncExternalStore } from "react";

const noSubscribe = () => () => {};

/** canOfferNativeApp, false while server rendering and hydrating. */
export function useCanOfferNativeApp(): boolean {
  return useSyncExternalStore(noSubscribe, canOfferNativeApp, () => false);
}
