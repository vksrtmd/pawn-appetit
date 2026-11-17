import { SegmentedControl, useMantineTheme } from "@mantine/core";
import { threadOptions } from "@/utils/engines";

export default function CoresSlider(props: {
  value: number;
  setValue: (v: number) => void;
  color?: string;
}) {
  const theme = useMantineTheme();
  const values = threadOptions(props.value, navigator.hardwareConcurrency);

  return (
    <SegmentedControl
      size="xs"
      color={props.color || theme.primaryColor}
      value={props.value.toString()}
      onChange={(v) => props.setValue(Number.parseInt(v, 10))}
      data={values.map((v) => v.toString())}
    />
  );
}
