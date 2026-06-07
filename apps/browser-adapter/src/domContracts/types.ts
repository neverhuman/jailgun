export interface DomTarCandidate {
  index: number;
  text: string;
  href: string;
  download: string;
  scope: 'assistant' | 'document';
  score: number;
  selector: string;
  tagName: string;
  role: string;
  aria: string;
  title: string;
  visible: boolean;
  clickable: boolean;
  assistantIndex: number | null;
  tarSources: string[];
  artifactSources?: string[];
  fileKind?: 'downloaded-archive' | 'downloaded-tex' | 'downloaded-file';
}

export interface DomArtifactConversationLink {
  index: number;
  url: string;
  href: string;
  text: string;
  aria: string;
  title: string;
  score: number;
  selector: string;
  tagName: string;
  conversationId: string;
  chapter: string;
  targetMatched: boolean;
  artifactSignals: string[];
}

export interface ToolPromptCandidate {
  index: number;
  signature: string;
  provider: 'github';
  action: 'read' | 'search' | 'write' | 'commit' | 'create-tree' | 'unknown';
  decision: 'deny' | 'allow-info';
  control: 'deny' | 'allow-info';
  label: string;
  context: string;
  score: number;
}

export interface RateLimitModalCandidate {
  dialogIndex: number;
  buttonIndex: number;
  buttonLabel: string;
  excerpt: string;
}

export interface RateLimitDismissalResult {
  detected: boolean;
  dismissed: boolean;
  excerpt: string;
  buttonLabel: string;
  reason?: string;
}

export interface RateLimitDismissalPage {
  evaluate: <T>(fn: () => T) => Promise<T>;
}

export type DismissablePopupKind = 'stay-on-page' | 'session-expired';

export interface DismissablePopupCandidate {
  kind: DismissablePopupKind;
  shouldClick: boolean;
  buttonLabel: string;
  excerpt: string;
}

export interface DismissablePopupOutcome extends DismissablePopupCandidate {
  detected: true;
  clicked: boolean;
  reason?: string;
}

export interface GitHubToolPromptClickResult {
  clicked: boolean;
  label: string;
  reason?: string;
}

export interface ClickablePage {
  locator: (selector: string) => {
    nth: (index: number) => {
      count?: () => Promise<number>;
      isVisible?: () => Promise<boolean>;
      isEnabled?: () => Promise<boolean>;
      textContent?: () => Promise<string | null>;
      getAttribute?: (name: string) => Promise<string | null>;
      click?: (options?: unknown) => Promise<void>;
    };
  };
}
