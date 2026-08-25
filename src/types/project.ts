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

export type OpenedFile = {
  name: string;
  path: string;
  content: string;
  savedContent: string;
};

export type FileReadResult = {
  content: string;
};
