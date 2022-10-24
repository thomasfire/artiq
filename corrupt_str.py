from artiq.experiment import *
from time import sleep

class BadString(EnvExperiment):
    def build(self):
        self.setattr_device('core')

    @rpc
    def get_string_rpc(self) -> TStr:
        return "a string new"

    @kernel
    def get_string(self) -> TStr:
        self.core.reset()
        my_str = self.get_string_rpc()
        # seems to cause the kernel panic
        print(my_str + my_str)
        return my_str[:]

    def run(self):
        print(self.get_string())