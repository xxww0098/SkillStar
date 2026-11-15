import { Text, View, mount, signal, type JSX } from "nsh";
import { Button } from "nsh/base";
import { IconSpin, IconSweep } from "./feedback";

// chrome/feedback 的无头装置：两枚字形各由一个开关驱动，断言「props 之后
// 变，组件跟着换形」（nsh 组件体只执行一次，换形必须落在 JSX 的 <If> 里；
// 帧动画的走向由真实宿主截图核对，这里只锁结构面的开关语义）。

const sweeping = signal(false);
const spinning = signal(false);

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8, padding: 12 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 8 }}>
        <Button testID="host-toggle-sweep" onPress={() => sweeping.set(!sweeping())}>
          <Text>sweep:{sweeping() ? "on" : "off"}</Text>
        </Button>
        <Button testID="host-toggle-spin" onPress={() => spinning.set(!spinning())}>
          <Text>spin:{spinning() ? "on" : "off"}</Text>
        </Button>
      </View>
      <View testID="host-sweep" style={{ display: "flex" }}>
        <IconSweep name="languages" size={14} color="#57534e" sweep={sweeping()} testID="sweep-beam" />
      </View>
      <View testID="host-spin" style={{ display: "flex" }}>
        <IconSpin name="languages" size={14} color="#57534e" spin={spinning()} testID="spin-frame" />
      </View>
    </View>
  );
}

mount(App);
