-- definitions for Lua 5.1 math library

--# assume global `math`:
--#     {
--#         `abs`: function(x: number) --> number;
--#         `acos`: function(x: number) --> number;
--#         `asin`: function(x: number) --> number;
--#         `atan`: function(x: number) --> number;
--#         `atan2`: function(y: number, x: number) --> number;
--#         `ceil`: function(x: number) --> integer;
--#         `cos`: function(x: number) --> number;
--#         `cosh`: function(x: number) --> number;
--#         `deg`: function(x: number) --> number;
--#         `exp`: function(x: number) --> number;
--#         `floor`: function(x: number) --> integer;
--#         `fmod`: function(x: number, y: number) --> number;
--#         `frexp`: function(x: number) --> (number, integer);
--#         `huge`: number;
--#         `ldexp`: function(m: number, e: integer) --> number;
--#         `log`: function(x: number) --> number;
--#         `log10`: function(x: number) --> number;
--#         -- TODO should really be
--#         --      `function(x: integer, integer...) --> integer &
--#         --       function(x: number, number...) --> number`
--#         `max`: function(x: number, number...) --> number;
--#         -- TODO should really be
--#         --      `function(x: integer, integer...) --> integer &
--#         --       function(x: number, number...) --> number`
--#         `min`: function(x: number, number...) --> number;
--#         `modf`: function(x: number) --> (integer, number);
--#         `pi`: number;
--#         `pow`: function(x: number, y: number) --> number;
--#         `rad`: function(x: number) --> number;
--#         -- TODO should really be
--#         --      `function() --> number & function(m: integer, n: integer?) --> integer`
--#         `random`: function(m: integer?, n: integer?) --> number;
--#         `randomseed`: function(x: integer);
--#         `sin`: function(x: number) --> number;
--#         `sinh`: function(x: number) --> number;
--#         `sqrt`: function(x: number) --> number;
--#         `tan`: function(x: number) --> number;
--#         `tanh`: function(x: number) --> number;
--#         ...
--#     }

