import type { FileEditorRequest, FileEditorReply } from '@contracts';
export interface FileEditorsApi {
  request(request: FileEditorRequest): Promise<FileEditorReply>;
  onChanged(listener: () => void): () => void;
  onShow(listener: () => void): () => void;
}
