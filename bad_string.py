from artiq.experiment import *


class BadString(EnvExperiment):
    def build(self):
        self.setattr_device('core')

    @rpc
    def get_string_rpc(self) -> TBytes:
        return bytes('a string', encoding='utf-8')

    @kernel
    def get_string(self) -> TBytes:
        self.core.reset()
        my_str = self.get_string_rpc()
        print(my_str)
        for b in my_str:
            print(int(b))
        return my_str[:]

    def run(self):
        my_str = self.get_string()
        print(my_str)
        for b in my_str:
            print(int(b))