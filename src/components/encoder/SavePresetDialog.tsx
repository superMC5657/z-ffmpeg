import PresetNameDialog, {
  type PresetNameDialogProps,
} from "@/components/preset/PresetNameDialog";

export type SavePresetDialogProps = Omit<PresetNameDialogProps, "mode">;

export default function SavePresetDialog({
  defaultName = "",
  onConfirm,
  onClose,
  open = true,
}: SavePresetDialogProps) {
  return (
    <PresetNameDialog
      mode="save"
      defaultName={defaultName}
      onConfirm={onConfirm}
      onClose={onClose}
      open={open}
    />
  );
}
