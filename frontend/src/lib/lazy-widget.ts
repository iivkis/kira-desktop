/**
 * A single WebKit page can host many widgets, but only the visible widget
 * should own DOM nodes, timers, subscriptions and Svelte reactive effects.
 * Serialize mount/unmount so quick hide/show never resurrects stale content.
 */
export interface LazyWidget<Instance> {
  mount(): Instance;
  unmount(instance: Instance): Promise<unknown> | void;
  onMounted(generation: number): void;
}

export function createLazyWidget<Instance>(
  load: () => Promise<LazyWidget<Instance>>,
  onError: (error: unknown) => void,
) {
  let desired: number | null = null;
  let active: { generation: number; instance: Instance; widget: LazyWidget<Instance> } | null = null;
  let draining = false;

  async function drain(): Promise<void> {
    if (draining) return;
    draining = true;
    try {
      while ((active?.generation ?? null) !== desired) {
        if (active) {
          const previous = active;
          active = null;
          // Svelte 5 unmount() cleans up onMount disposers, including
          // intervals and window listeners. Wait before mounting again.
          await previous.widget.unmount(previous.instance);
          continue;
        }
        if (desired === null) break;
        const generation = desired;
        const widget = await load();
        // A hidden or superseded widget is never mounted.
        if (desired !== generation) continue;
        const instance = widget.mount();
        active = { generation, instance, widget };
        widget.onMounted(generation);
      }
    } catch (error) {
      desired = null;
      onError(error);
    } finally {
      draining = false;
      if ((active?.generation ?? null) !== desired) void drain();
    }
  }

  function show(generation: number): void {
    if (!Number.isSafeInteger(generation) || generation <= 0 || generation > 0xffffffff) return;
    desired = generation;
    void drain();
  }

  function hide(): void {
    desired = null;
    void drain();
  }

  return { show, hide };
}
