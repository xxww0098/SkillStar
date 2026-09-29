/**
 * The Models page board: three columns of id + name.
 * Failures stay in the query result. The page still draws the column titles.
 */
import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { ModelsBoardDto } from "../../../types/generated/ModelsBoardDto";
import { modelsKeys } from "./keys";

export function useModelsBoard() {
  return useQuery<ModelsBoardDto>({
    queryKey: modelsKeys.board(),
    queryFn: () => tauriInvoke("get_models_board"),
  });
}
