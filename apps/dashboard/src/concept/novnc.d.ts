// Public noVNC API used by Jailgun; the upstream package ships no TypeScript types.
declare module '@novnc/novnc' {
  export default class RFB extends EventTarget {
    constructor(target: HTMLElement, url: string);
    scaleViewport: boolean;
    clipViewport: boolean;
    dragViewport: boolean;
    resizeSession: boolean;
    viewOnly: boolean;
    focus(options?: FocusOptions): void;
    disconnect(): void;
  }
}
