// Objective-C metadata laid out as clang lays it out, written in assembly so
// the fixture builds without Apple's tools: a class (Greeter, a subclass of
// NSObject) with instance and class methods, an ivar and a property; a
// category on NSObject with a relative ("small") method list; selector
// references; and code sending messages through objc_msgSend and an
// objc_msgSend$ stub. The methods are local symbols, so stripping the binary
// takes their names away: only the metadata still says what they are.
// Linked against libobjc.tbd (see scripts/build-fixtures.sh).
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

core::arch::global_asm!(
    r#"
    .section __TEXT,__text,regular,pure_instructions
    .globl _main
    .p2align 2
_main:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    adrp x8, _OBJC_CLASS_$_Greeter@PAGE
    add x0, x8, _OBJC_CLASS_$_Greeter@PAGEOFF
    adrp x1, l_selref_make@PAGE
    ldr x1, [x1, l_selref_make@PAGEOFF]
    bl _objc_msgSend
    adrp x1, l_selref_hello@PAGE
    ldr x1, [x1, l_selref_hello@PAGEOFF]
    bl _objc_msgSend
    bl _objc_msgSend$wave
    mov w0, #0
    ldp x29, x30, [sp], #16
    ret

    .p2align 2
_Greeter_hello:
    mov w0, #1
    ret

    .p2align 2
_Greeter_greetWith_times:
    mov w0, #2
    ret

    .p2align 2
_Greeter_make:
    mov w0, #3
    ret

    .p2align 2
_NSObject_Extras_wave:
    mov w0, #4
    ret

    .section __TEXT,__objc_methname,cstring_literals
l_sel_hello: .asciz "hello"
l_sel_greet: .asciz "greetWith:times:"
l_sel_make: .asciz "make"
l_sel_wave: .asciz "wave"
l_ivar_name: .asciz "_name"

    .section __TEXT,__objc_classname,cstring_literals
l_class_name: .asciz "Greeter"
l_category_name: .asciz "Extras"

    .section __TEXT,__objc_methtype,cstring_literals
l_type_void: .asciz "v16@0:8"
l_type_object: .asciz "@16@0:8"
l_type_greet: .asciz "v28@0:8@16i24"
l_type_string: .asciz "@\"NSString\""

    .section __TEXT,__cstring,cstring_literals
l_prop_name: .asciz "name"
l_prop_attributes: .asciz "T@\"NSString\",&,N,V_name"

    .section __DATA,__objc_ivar
    .p2align 2
_OBJC_IVAR_$_Greeter._name:
    .long 8

    .section __DATA,__objc_const
    .p2align 3
l_instance_methods:
    .long 24
    .long 2
    .quad l_sel_hello, l_type_void, _Greeter_hello
    .quad l_sel_greet, l_type_greet, _Greeter_greetWith_times
l_class_methods:
    .long 24
    .long 1
    .quad l_sel_make, l_type_object, _Greeter_make
l_ivars:
    .long 32
    .long 1
    .quad _OBJC_IVAR_$_Greeter._name, l_ivar_name, l_type_string
    .long 3
    .long 8
l_properties:
    .long 16
    .long 1
    .quad l_prop_name, l_prop_attributes
l_class_ro:
    .long 0
    .long 8
    .long 16
    .long 0
    .quad 0
    .quad l_class_name
    .quad l_instance_methods
    .quad 0
    .quad l_ivars
    .quad 0
    .quad l_properties
l_metaclass_ro:
    .long 1
    .long 40
    .long 40
    .long 0
    .quad 0
    .quad l_class_name
    .quad l_class_methods
    .quad 0
    .quad 0
    .quad 0
    .quad 0
l_category_methods:
    .long 0x8000000c
    .long 1
    .long l_selref_wave - (l_category_methods + 8)
    .long l_type_void - (l_category_methods + 12)
    .long _NSObject_Extras_wave - (l_category_methods + 16)
l_category:
    .quad l_category_name
    .quad _OBJC_CLASS_$_NSObject
    .quad l_category_methods
    .quad 0
    .quad 0
    .quad 0

    .section __DATA,__objc_data
    .globl _OBJC_CLASS_$_Greeter
    .globl _OBJC_METACLASS_$_Greeter
    .p2align 3
_OBJC_CLASS_$_Greeter:
    .quad _OBJC_METACLASS_$_Greeter
    .quad _OBJC_CLASS_$_NSObject
    .quad __objc_empty_cache
    .quad 0
    .quad l_class_ro
_OBJC_METACLASS_$_Greeter:
    .quad _OBJC_METACLASS_$_NSObject
    .quad _OBJC_METACLASS_$_NSObject
    .quad __objc_empty_cache
    .quad 0
    .quad l_metaclass_ro

    .section __DATA,__objc_classlist,regular,no_dead_strip
    .p2align 3
l_classlist:
    .quad _OBJC_CLASS_$_Greeter

    .section __DATA,__objc_catlist,regular,no_dead_strip
    .p2align 3
l_catlist:
    .quad l_category

    .section __DATA,__objc_selrefs,literal_pointers,no_dead_strip
    .p2align 3
l_selref_hello:
    .quad l_sel_hello
l_selref_make:
    .quad l_sel_make
l_selref_wave:
    .quad l_sel_wave

    .section __DATA,__objc_imageinfo,regular,no_dead_strip
    .long 0
    .long 64
"#
);
