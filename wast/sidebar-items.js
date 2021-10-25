initSidebarItems({"enum":[["AliasSource",""],["AssertExpression","An expression that is valid inside an `assert_return` directive."],["CustomPlace","Possible locations to place a custom section within a module."],["CustomPlaceAnchor","Known sections that custom sections can be placed relative to."],["DataKind","Different kinds of data segments, either passive or active."],["DataVal","Differnet ways the value of a data segment can be defined."],["ElemKind","Different ways to define an element segment in an mdoule."],["ElemPayload","Different ways to define the element segment payload in a module."],["EventType","Listing of various types of events that can be defined in a wasm module."],["ExportKind","Different kinds of elements that can be exported from a WebAssembly module, contained in an [`Export`]."],["FuncKind","Possible ways to define a function in the text format."],["GlobalKind","Different kinds of globals that can be defined in a module."],["HeapType","A heap type for a reference type"],["Index","A reference to another item in a wasm module."],["InstanceKind","Possible ways to define a instance in the text format."],["Instruction","A listing of all WebAssembly instructions that can be in a module that this crate currently parses."],["ItemKind",""],["ItemRef","Parses `(func $foo)`"],["MemoryKind","Different syntactical ways a memory can be defined in a module."],["MemoryType","Configuration for a memory of a wasm module"],["ModuleField","A listing of all possible fields that can make up a WebAssembly module."],["ModuleKind","The different kinds of ways to define a module."],["NanPattern","Either a NaN pattern (`nan:canonical`, `nan:arithmetic`) or a value of type `T`."],["NestedModuleKind","Possible ways to define a nested module in the text format."],["QuoteModule",""],["StorageType","The types of values that may be used in a struct or array."],["TableKind","Different ways to textually define a table."],["TypeDef","A definition of a type."],["V128Const","Different ways to specify a `v128.const` instruction"],["V128Pattern","A version of `V128Const` that allows `NanPattern`s."],["ValType","The value types for a wasm module."],["WastDirective","The different kinds of directives found in a `*.wast` file."],["WastExecute",""]],"macro":[["annotation","A macro, like [`custom_keyword`], to create a type which can be used to parse/peek annotation directives."],["annotation","A macro, like [`custom_keyword`], to create a type which can be used to parse/peek annotation directives."],["custom_keyword","A macro to create a custom keyword parser."],["custom_keyword","A macro to create a custom keyword parser."],["custom_reserved","A macro for defining custom reserved symbols."],["custom_reserved","A macro for defining custom reserved symbols."]],"mod":[["annotation","Common annotations used to parse WebAssembly text files."],["kw","Common keyword used to parse WebAssembly text files."],["lexer","Definition of a lexer for the WebAssembly text format."],["parser","Traits for parsing the WebAssembly Text format"]],"struct":[["Alias","An `alias` statement used to juggle indices with nested modules."],["ArrayType","An array type with fields."],["BlockType","Extra information associated with block-related instructions."],["BrTableIndices","Extra information associated with the `br_table` instruction."],["CallIndirect","Extra data associated with the `call_indirect` instruction."],["Custom","A wasm custom section within a module."],["Data","A `data` directive in a WebAssembly module."],["Elem","An `elem` segment in a WebAssembly module."],["Error","A convenience error type to tie together all the detailed errors produced by this crate."],["Event","A WebAssembly event directive, part of the exception handling proposal."],["Export","A entry in a WebAssembly module’s export section."],["ExportType","The type of an exported item from a module or instance."],["Expression","An expression, or a list of instructions, in the WebAssembly text format."],["Float32","A parsed floating-point type"],["Float64","A parsed floating-point type"],["Func","A WebAssembly function to be inserted into a module."],["FuncBindType","Extra information associated with the func.bind instruction."],["FunctionType","A function type with parameters and results."],["FunctionTypeNoNames","A function type with parameters and results."],["Global","A WebAssembly global in a module"],["GlobalType","Type for a `global` in a wasm module"],["I8x16Shuffle","Lanes being shuffled in the `i8x16.shuffle` instruction"],["Id","An identifier in a WebAssembly module, prefixed by `$` in the textual format."],["Import","An `import` statement and entry in a WebAssembly module."],["IndexOrRef","Convenience structure to parse `$f` or `(item $f)`."],["InlineExport","A listing of inline `(export \"foo\")` statements on a WebAssembly item in its textual format."],["InlineImport","A listing of a inline `(import \"foo\")` statement."],["Instance","A nested WebAssembly instance to be created as part of a module."],["InstanceArg","Arguments to the `instantiate` instruction"],["InstanceType","A type for a nested instance"],["ItemSig",""],["LParen","A convenience type to use with `Parser::peek` to see if the next token is an s-expression."],["LaneArg","Payload for lane-related instructions. Unsigned with no + prefix."],["LetType","Extra information associated with the let instruction."],["Limits","Min/max limits used for tables/memories."],["Limits64","Min/max limits used for 64-bit memories"],["LoadOrStoreLane","Extra data associated with the `loadN_lane` and `storeN_lane` instructions."],["Local","A local for a `func` or `let` instruction."],["MemArg","Payload for memory-related instructions indicating offset/alignment of memory accesses."],["Memory","A defined WebAssembly memory instance inside of a module."],["MemoryArg","Extra data associated with unary memory instructions."],["MemoryCopy","Extra data associated with the `memory.copy` instruction"],["MemoryInit","Extra data associated with the `memory.init` instruction"],["Module","A parsed WebAssembly module."],["ModuleType","A type for a nested module"],["NameAnnotation","An `@name` annotation in source, currently of the form `@name \"foo\"`"],["Names","Representation of the results of name resolution for a module."],["NestedModule","A nested WebAssembly nested module to be created as part of a module."],["RefType","A reference type in a wasm module."],["SelectTypes","Payload of the `select` instructions"],["Span","A position in the original source stream, used to render errors."],["StructAccess","Extra data associated with the `struct.get/set` instructions"],["StructField","A field of a struct type."],["StructType","A struct type with fields."],["Table","A WebAssembly `table` directive in a module."],["TableArg","Extra data associated with unary table instructions."],["TableCopy","Extra data associated with the `table.copy` instruction."],["TableInit","Extra data associated with the `table.init` instruction"],["TableType","Configuration for a table of a wasm mdoule"],["Type","A type declaration in a module"],["TypeUse","A reference to a type defined in this module."],["Wast","A parsed representation of a `*.wast` file."],["WastInvoke",""],["Wat","A `*.wat` file parser, or a parser for one parenthesized module."]]});