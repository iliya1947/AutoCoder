export type ProjectNode = {
  name: string;
  path: string;
  kind: "directory" | "file";
  children: ProjectNode[];
};

export type ProjectTree = {
  name: string;
  children: ProjectNode[];
};

export type OpenProjectResult = {
  project: ProjectTree;
  sessionChanged: boolean;
};

export type RestoredWorkspace = {
  project: ProjectTree;
  openFile: { path: string; content: string } | null;
};

export type RefreshProjectResult = {
  project: ProjectTree;
  openFileContent: string | null;
};

export type OpenedFile = {
  name: string;
  path: string;
  content: string;
  savedContent: string;
  /** False when an external tool removed the file while a dirty buffer is kept. */
  existsOnDisk?: boolean;
};

export type FileReadResult = {
  content: string;
};
