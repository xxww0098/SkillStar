import { useState, useCallback, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, FolderPlus, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

interface CreateLocalSkillModalProps {
  open: boolean;
  onClose: () => void;
  onCreateLocalSkill: (name: string) => Promise<unknown>;
  existingSkillNames: Set<string>;
}

const NAME_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

export function CreateLocalSkillModal({
  open,
  onClose,
  onCreateLocalSkill,
  existingSkillNames,
}: CreateLocalSkillModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setName("");
      setError(null);
      setCreating(false);
      // Auto-focus input after animation
      setTimeout(() => inputRef.current?.focus(), 150);
    }
  }, [open]);

  const validate = useCallback(
    (value: string): string | null => {
      const trimmed = value.trim();
      if (!trimmed) return t("mySkills.skillNameRequired");
      if (!NAME_PATTERN.test(trimmed)) return t("mySkills.skillNameInvalid");
      if (existingSkillNames.has(trimmed)) return t("mySkills.skillNameExists");
      return null;
    },
    [existingSkillNames, t]
  );

  const handleSubmit = async () => {
    const trimmed = name.trim();
    const validationError = validate(trimmed);
    if (validationError) {
      setError(validationError);
      return;
    }

    setCreating(true);
    setError(null);
    try {
      await onCreateLocalSkill(trimmed);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !creating) {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === "Escape") {
      onClose();
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
          onClick={onClose}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 10 }}
            transition={{ type: "spring", bounce: 0.15, duration: 0.3 }}
            className="w-[420px] bg-card border border-border rounded-2xl shadow-2xl p-6"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="flex items-center justify-between mb-5">
              <div className="flex items-center gap-3">
                <div className="w-9 h-9 rounded-xl bg-indigo-500/10 flex items-center justify-center">
                  <FolderPlus className="w-4.5 h-4.5 text-indigo-400" />
                </div>
                <h2 className="text-heading-sm">{t("mySkills.createLocalSkill")}</h2>
              </div>
              <button
                onClick={onClose}
                className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Name input */}
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t("mySkills.skillNameLabel")}
              </label>
              <Input
                ref={inputRef}
                value={name}
                onChange={(e) => {
                  setName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""));
                  setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder={t("mySkills.skillNamePlaceholder")}
                className="font-mono"
                disabled={creating}
              />
              {error && (
                <p className="text-xs text-destructive">{error}</p>
              )}
              <p className="text-xs text-muted-foreground/70">
                Stored in <code className="text-[11px] bg-muted/50 px-1 py-0.5 rounded">~/.agents/skills-local/{name || "..."}/</code>
              </p>
            </div>

            {/* Actions */}
            <div className="flex justify-end gap-2 mt-6">
              <Button variant="ghost" onClick={onClose} disabled={creating}>
                {t("common.cancel")}
              </Button>
              <Button
                onClick={handleSubmit}
                disabled={creating || !name.trim()}
                className="min-w-[80px]"
              >
                {creating ? (
                  <>
                    <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                    {t("mySkills.creating")}
                  </>
                ) : (
                  t("mySkills.create")
                )}
              </Button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
