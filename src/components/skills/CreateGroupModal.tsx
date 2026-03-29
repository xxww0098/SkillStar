import { useState, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Plus, Search, Check, AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { cn } from "../../lib/utils";
import type { Skill } from "../../types";

const EMOJI_OPTIONS = [
  "💻", "🚀", "🎨", "🔧", "📦", "🧪", "📊", "🔐",
  "🌐", "📝", "⚡", "🤖", "🛠️", "📱", "🎯", "🧩",
];

interface CreateGroupModalProps {
  open: boolean;
  onClose: () => void;
  availableSkills: Skill[];
  initialSkills?: string[];
  initialName?: string;
  initialDescription?: string;
  initialIcon?: string;
  mode?: "create" | "edit";
  existingNames?: string[];
  onSave: (
    name: string,
    description: string,
    icon: string,
    skills: string[]
  ) => Promise<void>;
}

export function CreateGroupModal({
  open: isOpen,
  onClose,
  availableSkills,
  initialSkills = [],
  initialName = "",
  initialDescription = "",
  initialIcon = "💻",
  mode = "create",
  existingNames = [],
  onSave,
}: CreateGroupModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState(initialDescription);
  const [icon, setIcon] = useState(initialIcon);
  const [selectedSkills, setSelectedSkills] = useState<string[]>(initialSkills);
  const [saving, setSaving] = useState(false);
  const [skillSearch, setSkillSearch] = useState("");
  const [emojiOpen, setEmojiOpen] = useState(false);
  const emojiRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen) {
      setName(initialName);
      setDescription(initialDescription);
      setIcon(initialIcon);
      setSelectedSkills(initialSkills);
      setSkillSearch("");
      setEmojiOpen(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  useEffect(() => {
    if (!emojiOpen) return;
    const handler = (e: MouseEvent) => {
      if (emojiRef.current && !emojiRef.current.contains(e.target as Node)) {
        setEmojiOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [emojiOpen]);

  const handleClose = () => onClose();

  const handleSave = async () => {
    if (!name.trim() || selectedSkills.length === 0) return;
    setSaving(true);
    try {
      await onSave(name.trim(), description.trim(), icon, selectedSkills);
      handleClose();
    } catch (e) {
      console.error("Save failed:", e);
    } finally {
      setSaving(false);
    }
  };

  const toggleSkill = (skillName: string) => {
    setSelectedSkills((prev) =>
      prev.includes(skillName)
        ? prev.filter((s) => s !== skillName)
        : [...prev, skillName]
    );
  };

  const filteredSkills = availableSkills.filter(
    (s) =>
      !skillSearch ||
      s.name.toLowerCase().includes(skillSearch.toLowerCase()) ||
      s.description.toLowerCase().includes(skillSearch.toLowerCase())
  );

  const isDuplicateName =
    name.trim() !== initialName.trim() && existingNames.includes(name.trim());
  const canSave = name.trim().length > 0 && selectedSkills.length > 0 && !isDuplicateName;

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-lg z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              {/* Header */}
              <div className="flex items-center justify-between px-6 pt-4 pb-0 shrink-0">
                <h2 className="text-heading-sm">
                  {mode === "edit" ? t("createGroupModal.editGroup") : t("createGroupModal.newGroup")}
                </h2>
                <button
                  onClick={handleClose}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              {/* Body */}
              <div className="flex-1 overflow-y-auto px-6 py-4 space-y-3">
                {/* Icon + Name */}
                <div className="space-y-1.5" ref={emojiRef}>
                  <div className="relative flex items-end gap-3">
                    <button
                      onClick={() => setEmojiOpen(!emojiOpen)}
                      className={cn(
                        "w-11 h-11 rounded-xl border flex items-center justify-center text-xl shrink-0 transition-all cursor-pointer",
                        emojiOpen
                          ? "border-primary/50 bg-primary/5"
                          : "border-border hover:bg-muted",
                        isDuplicateName && "border-destructive/50 bg-destructive/5"
                      )}
                    >
                      {icon}
                    </button>

                    <AnimatePresence>
                      {emojiOpen && (
                        <motion.div
                          initial={{ opacity: 0, y: -4 }}
                          animate={{ opacity: 1, y: 0 }}
                          exit={{ opacity: 0, y: -4 }}
                          transition={{ duration: 0.12 }}
                          className="absolute top-full left-0 mt-2 p-2 rounded-xl border border-border bg-card shadow-lg grid grid-cols-4 gap-0.5 z-10 w-[164px]"
                        >
                          {EMOJI_OPTIONS.map((emoji) => (
                            <button
                              key={emoji}
                              onClick={() => {
                                setIcon(emoji);
                                setEmojiOpen(false);
                              }}
                              className={cn(
                                "w-9 h-9 rounded-lg flex items-center justify-center text-lg transition-colors cursor-pointer",
                                icon === emoji
                                  ? "bg-primary/10"
                                  : "hover:bg-muted"
                              )}
                            >
                              {emoji}
                            </button>
                          ))}
                        </motion.div>
                      )}
                    </AnimatePresence>

                    <div className="flex-1 relative">
                      <Input
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder={t("createGroupModal.groupName")}
                        className={cn(isDuplicateName && "border-destructive/80 focus-visible:ring-destructive/30 text-destructive")}
                      />
                    </div>
                  </div>
                  
                  <AnimatePresence>
                    {isDuplicateName && (
                      <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: "auto" }}
                        exit={{ opacity: 0, height: 0 }}
                        className="overflow-hidden"
                      >
                        <p className="text-xs text-destructive font-medium flex items-center gap-1.5 pl-[56px] pt-0.5 pb-0.5">
                          <AlertCircle className="w-3.5 h-3.5" />
                          {t("createGroupModal.nameExists", "名称已存在")}
                        </p>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>

                {/* Description */}
                <Input
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder={t("createGroupModal.description")}
                />

                {/* Skills section */}
                <div className="pt-1">
                  {/* Selected pills */}
                  <AnimatePresence mode="popLayout">
                    {selectedSkills.length > 0 && (
                      <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: "auto" }}
                        exit={{ opacity: 0, height: 0 }}
                        className="flex flex-wrap gap-1 mb-2"
                      >
                        {selectedSkills.map((skillName) => (
                          <motion.button
                            key={skillName}
                            layout
                            initial={{ scale: 0.85, opacity: 0 }}
                            animate={{ scale: 1, opacity: 1 }}
                            exit={{ scale: 0.85, opacity: 0 }}
                            transition={{ duration: 0.12 }}
                            onClick={() => toggleSkill(skillName)}
                            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-primary/10 text-primary text-xs font-medium hover:bg-primary/15 transition-colors cursor-pointer"
                          >
                            {skillName}
                            <X className="w-2.5 h-2.5 opacity-50" />
                          </motion.button>
                        ))}
                      </motion.div>
                    )}
                  </AnimatePresence>

                  {/* Search */}
                  <div className="relative mb-1.5">
                    <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground pointer-events-none" />
                    <Input
                      value={skillSearch}
                      onChange={(e) => setSkillSearch(e.target.value)}
                      placeholder={t("createGroupModal.searchSkills")}
                      className="pl-8 h-8 text-sm"
                    />
                  </div>

                  {/* Skill list */}
                  <div className="max-h-40 overflow-y-auto rounded-lg">
                    {filteredSkills.length > 0 ? (
                      <div className="space-y-0.5">
                        {filteredSkills.map((skill) => {
                          const isSelected = selectedSkills.includes(skill.name);
                          return (
                            <button
                              key={skill.name}
                              onClick={() => toggleSkill(skill.name)}
                              className={cn(
                                "w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer",
                                isSelected ? "bg-primary/5" : "hover:bg-muted"
                              )}
                            >
                              <div
                                className={cn(
                                  "w-4 h-4 rounded border-[1.5px] flex items-center justify-center shrink-0 transition-all",
                                  isSelected
                                    ? "bg-primary border-primary"
                                    : "border-muted-foreground/30"
                                )}
                              >
                                {isSelected && (
                                  <Check className="w-2.5 h-2.5 text-white" strokeWidth={3} />
                                )}
                              </div>

                              <span className={cn(
                                "text-[13px] truncate",
                                isSelected ? "text-primary font-medium" : "text-foreground"
                              )}>
                                {skill.name}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    ) : (
                      <div className="py-6 text-center text-sm text-muted-foreground">
                        {t("createGroupModal.noSkillsFound")}
                      </div>
                    )}
                  </div>
                </div>
              </div>

              {/* Footer */}
              <div className="flex items-center justify-end gap-2 px-6 py-3.5 border-t border-border/60 shrink-0">
                <Button variant="ghost" size="sm" onClick={handleClose}>
                   {t("createGroupModal.cancel")}
                </Button>
                <Button
                  size="sm"
                  onClick={handleSave}
                  disabled={!canSave || saving}
                >
                  {saving ? (
                    <span className="flex items-center gap-1.5">
                      <span className="w-3.5 h-3.5 border-2 border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin" />
                       {t("createGroupModal.saving")}
                    </span>
                  ) : mode === "edit" ? (
                    t("createGroupModal.save")
                  ) : (
                    <span className="flex items-center gap-1.5">
                      <Plus className="w-3.5 h-3.5" />
                       {t("createGroupModal.create")}
                    </span>
                  )}
                </Button>
              </div>
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
