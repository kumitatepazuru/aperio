from aperio.item_structures import GenerateStructure

def make_link_stub(original_id: str) -> GenerateStructure:
    """`link_id`が`original_id`を指すだけの、実体を持たない軽量な`GenerateStructure`を作る。
    `link_id`が設定されている場合、name/display_name/parametersは内部で無視されるため
    空のプレースホルダで良い。"""
def build_link_prefix(item, current_id: str) -> tuple[GenerateStructure, list[GenerateStructure]]:
    """`item`(`ItemStructure.Video`/`ItemStructure.Audio`)の`object`から`current_id`の
    手前まで(`current_id`自体は含まない)を、元のエントリをそれぞれ`link_id`で参照する
    軽量スタブの列として切り出す。

    戻り値は`(object_stub, effect_stubs)`。`current_id`が`object`自身のidの場合、
    `effect_stubs`は空になる。`current_id`が見つからない場合は`effects`全体を切り出す。
    """
